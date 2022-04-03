use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::future::Either;
use tokio::sync::{Mutex, Notify, RwLock, RwLockWriteGuard};

use crate::config::*;
use crate::utils::*;

/// Creates exporter service and metrics writer
pub async fn create_exporter(
    config: Option<Config>,
) -> Result<(Arc<MetricsExporter>, MetricsWriter)> {
    let (completion_trigger, completion_signal) = trigger();

    let handle = Arc::new(MetricsExporterHandle::default());

    let exporter = Arc::new(MetricsExporter {
        handle: handle.clone(),
        running_endpoint: Default::default(),
        completion_trigger,
        completion_signal,
    });

    exporter
        .reload(config)
        .await
        .context("Failed to create metrics exporter")?;

    Ok((exporter, MetricsWriter { handle }))
}

/// Prometheus metrics exporter
pub struct MetricsExporter {
    /// Shared exporter state
    handle: Arc<MetricsExporterHandle>,

    /// Triggers and signals for running exporter service
    running_endpoint: Mutex<Option<RunningEndpoint>>,

    completion_trigger: Trigger,
    completion_signal: TriggerReceiver,
}

impl MetricsExporter {
    /// Updates exporter config if some, disables service otherwise
    pub async fn reload(&self, config: Option<Config>) -> Result<()> {
        let mut running_endpoint = self.running_endpoint.lock().await;

        // Stop running service
        if let Some(endpoint) = running_endpoint.take() {
            // Initiate completion
            endpoint.completion_trigger.trigger();
            // And wait until it stops completely
            endpoint.stopped_signal.await;
        }

        let config = match config {
            Some(config) => config,
            None => {
                log::info!("Disable metrics exporter");
                self.handle.interval_sec.store(0, Ordering::Release);
                self.handle.new_config_notify.notify_waiters();
                return Ok(());
            }
        };

        // Create http service
        let server = hyper::Server::try_bind(&config.listen_address)
            .context("Failed to bind metrics exporter server port")?;

        let path = config.metrics_path.clone();
        let buffers = self.handle.buffers.clone();

        let make_service = hyper::service::make_service_fn(move |_| {
            let path = path.clone();
            let buffers = buffers.clone();

            async move {
                Ok::<_, Infallible>(hyper::service::service_fn(move |req| {
                    // Allow only GET metrics_path
                    if req.method() != hyper::Method::GET || req.uri() != path.as_str() {
                        return Either::Left(futures::future::ready(
                            hyper::Response::builder()
                                .status(hyper::StatusCode::NOT_FOUND)
                                .body(hyper::Body::empty()),
                        ));
                    }

                    let buffers = buffers.clone();

                    // Prepare metrics response
                    Either::Right(async move {
                        let data = buffers.get_metrics().await;
                        hyper::Response::builder()
                            .header("Content-Type", "text/plain; charset=UTF-8")
                            .body(hyper::Body::from(data))
                    })
                }))
            }
        });

        // Use completion signal as graceful shutdown notify
        let completion_signal = self.completion_signal.clone();
        let (stopped_trigger, stopped_signal) = trigger();
        let (local_completion_trigger, local_completion_signal) = trigger();

        log::info!("Metrics exporter started");

        // Spawn server
        tokio::spawn(async move {
            let server = server
                .serve(make_service)
                .with_graceful_shutdown(async move {
                    futures::future::select(completion_signal, local_completion_signal).await;
                });

            if let Err(e) = server.await {
                log::error!("Metrics exporter stopped: {:?}", e);
            } else {
                log::info!("Metrics exporter stopped");
            }

            // Notify when server is stopped
            stopped_trigger.trigger();
        });

        // Update running endpoint
        *running_endpoint = Some(RunningEndpoint {
            completion_trigger: local_completion_trigger,
            stopped_signal,
        });

        // Update interval and notify waiters
        self.handle
            .interval_sec
            .store(config.collection_interval_sec, Ordering::Release);
        self.handle.new_config_notify.notify_waiters();

        // Done
        Ok(())
    }
}

impl Drop for MetricsExporter {
    fn drop(&mut self) {
        // Trigger server shutdown on drop
        self.completion_trigger.trigger();
    }
}

/// Prometheus metrics writer
pub struct MetricsWriter {
    /// Shared exporter state
    handle: Arc<MetricsExporterHandle>,
}

impl MetricsWriter {
    pub fn spawn<F>(self, mut f: F)
    where
        for<'a> F: FnMut(&mut MetricsBuffer<'a>) + Send + 'static,
    {
        let handle = Arc::downgrade(&self.handle);

        tokio::spawn(async move {
            loop {
                let handle = match handle.upgrade() {
                    Some(handle) => {
                        f(&mut handle.buffers().acquire_buffer().await);
                        handle
                    }
                    None => return,
                };

                handle.wait().await;
            }
        });
    }
}

#[derive(Default)]
struct MetricsExporterHandle {
    buffers: Arc<Buffers>,
    interval_sec: AtomicU64,
    new_config_notify: Notify,
}

impl MetricsExporterHandle {
    fn buffers(&self) -> &Arc<Buffers> {
        &self.buffers
    }

    async fn wait(&self) {
        loop {
            // Start waiting config change
            let new_config = self.new_config_notify.notified();
            // Load current interval
            let current_interval = self.interval_sec.load(Ordering::Acquire);

            // Zero interval means that there is no current config and we should not
            // do anything but waiting new value
            if current_interval == 0 {
                new_config.await;
                continue;
            }

            tokio::select! {
                // Wait current interval
                _ = tokio::time::sleep(Duration::from_secs(current_interval)) => return,
                // Or resolve earlier on new non-zero interval
                _ = new_config => {
                    if self.interval_sec.load(Ordering::Acquire) > 0 {
                        return
                    } else {
                        // Wait for the new config otherwise
                        continue
                    }
                },
            }
        }
    }
}

struct RunningEndpoint {
    completion_trigger: Trigger,
    stopped_signal: TriggerReceiver,
}

#[derive(Default)]
struct Buffers {
    data: [RwLock<String>; BUFFER_COUNT],
    current_buffer: AtomicUsize,
}

impl Buffers {
    async fn acquire_buffer<'a, 's>(&'s self) -> MetricsBuffer<'a>
    where
        's: 'a,
    {
        let next_buffer = (self.current_buffer.load(Ordering::Acquire) + 1) % BUFFER_COUNT;
        let mut buffer_guard = self.data[next_buffer].write().await;
        buffer_guard.clear();
        MetricsBuffer {
            current_buffer: &self.current_buffer,
            next_buffer,
            buffer_guard,
        }
    }

    async fn get_metrics(&self) -> String {
        self.data[self.current_buffer.load(Ordering::Acquire)]
            .read()
            .await
            .clone()
    }
}

pub struct MetricsBuffer<'a> {
    current_buffer: &'a AtomicUsize,
    next_buffer: usize,
    buffer_guard: RwLockWriteGuard<'a, String>,
}

impl<'a> MetricsBuffer<'a> {
    pub fn write<T>(&mut self, metrics: T) -> &mut Self
    where
        T: std::fmt::Display,
    {
        self.buffer_guard.push_str(&metrics.to_string());
        self
    }
}

impl<'a> Drop for MetricsBuffer<'a> {
    fn drop(&mut self) {
        self.current_buffer
            .store(self.next_buffer, Ordering::Release);
    }
}

const BUFFER_COUNT: usize = 2;
