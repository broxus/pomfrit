use std::borrow::Borrow;
use std::fmt::Write;

pub trait DisplayPrometheusExt<'b> {
    fn begin_metric<'a>(&'a mut self, name: &str) -> PrometheusFormatter<'a, 'b>;
}

impl<'b> DisplayPrometheusExt<'b> for std::fmt::Formatter<'b> {
    fn begin_metric<'a>(&'a mut self, name: &str) -> PrometheusFormatter<'a, 'b> {
        PrometheusFormatter::new(self, name)
    }
}

pub struct PrometheusFormatter<'a, 'b> {
    fmt: &'a mut std::fmt::Formatter<'b>,
    result: std::fmt::Result,
    has_labels: bool,
}

impl<'a, 'b> PrometheusFormatter<'a, 'b> {
    pub fn new(fmt: &'a mut std::fmt::Formatter<'b>, name: &str) -> Self {
        let result = fmt.write_str(name);
        Self {
            fmt,
            result,
            has_labels: false,
        }
    }

    #[inline]
    pub fn label_opt<N, V>(self, name: N, value: impl Borrow<Option<V>>) -> Self
    where
        N: std::fmt::Display,
        V: std::fmt::Display,
    {
        if let Some(value) = value.borrow() {
            self.label(name, value)
        } else {
            self
        }
    }

    #[inline]
    pub fn label<N, V>(self, name: N, value: V) -> Self
    where
        N: std::fmt::Display,
        V: std::fmt::Display,
    {
        let PrometheusFormatter {
            fmt,
            result,
            has_labels,
        } = self;

        let result = result.and_then(|_| {
            fmt.write_char(if has_labels { ',' } else { '{' })?;
            name.fmt(fmt)?;
            fmt.write_str("=\"")?;
            value.fmt(fmt)?;
            fmt.write_char('\"')
        });

        Self {
            fmt,
            result,
            has_labels: true,
        }
    }

    #[inline]
    pub fn value<T>(self, value: impl std::borrow::Borrow<T>) -> std::fmt::Result
    where
        T: num_traits::Num + std::fmt::Display,
    {
        self.result.and_then(|_| {
            if self.has_labels {
                self.fmt.write_str("} ")?;
            } else {
                self.fmt.write_char(' ')?;
            }
            value.borrow().fmt(self.fmt)?;
            self.fmt.write_char('\n')
        })
    }
}
