use std::{
    backtrace::Backtrace,
    error::Error,
    sync::atomic::{AtomicBool, Ordering},
};

static CAPTURE_BACKTRACES: AtomicBool = AtomicBool::new(false);

pub const EXCEPTION_TARGET: &str = "telemetry::exception";
pub const SUMMARY_TARGET: &str = "telemetry::error_summary";

pub fn enable_backtraces() {
    CAPTURE_BACKTRACES.store(true, Ordering::Relaxed);
}

pub fn record(
    category: &'static str,
    operation: &'static str,
    error: &(dyn Error + 'static),
    context: impl AsRef<str>,
) {
    let message = error.to_string();
    let chain = error_chain(error);

    tracing::error!(
        target: SUMMARY_TARGET,
        error_category = category,
        operation,
        error_message = message.as_str(),
        error_context = context.as_ref(),
        "operation failed"
    );

    record_exception(category, operation, &message, &chain, context.as_ref());
}

pub fn record_message(
    category: &'static str,
    operation: &'static str,
    message: impl AsRef<str>,
    context: impl AsRef<str>,
) {
    let message = message.as_ref();
    tracing::error!(
        target: SUMMARY_TARGET,
        error_category = category,
        operation,
        error_message = message,
        error_context = context.as_ref(),
        "operation failed"
    );

    record_exception(category, operation, message, "", context.as_ref());
}

pub fn panic(message: &str, location: &str) {
    tracing::error!(
        target: SUMMARY_TARGET,
        error_category = "panic",
        operation = "process",
        error_message = message,
        error_context = location,
        "process panicked"
    );

    record_exception("panic", "process", message, "", location);
}

fn error_chain(error: &(dyn Error + 'static)) -> String {
    let mut messages = Vec::new();
    let mut current = error.source();

    while let Some(source) = current {
        messages.push(source.to_string());
        current = source.source();
    }

    messages.join(": ")
}

fn record_exception(
    category: &'static str,
    operation: &'static str,
    message: &str,
    chain: &str,
    context: &str,
) {
    if !CAPTURE_BACKTRACES.load(Ordering::Relaxed) {
        return;
    }

    tracing::error!(
        target: EXCEPTION_TARGET,
        error_category = category,
        operation,
        error_message = message,
        error_chain = chain,
        error_context = context,
        exception_stacktrace = %Backtrace::force_capture(),
        "exception"
    );
}

#[cfg(test)]
mod tests {
    use std::{error::Error, fmt};

    use super::error_chain;

    #[derive(Debug)]
    struct TestError {
        message: &'static str,
        source: Option<Box<TestError>>,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.message)
        }
    }

    impl Error for TestError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source
                .as_deref()
                .map(|source| source as &(dyn Error + 'static))
        }
    }

    #[test]
    fn formats_source_chain_from_nearest_to_root() {
        let error = TestError {
            message: "outer",
            source: Some(Box::new(TestError {
                message: "middle",
                source: Some(Box::new(TestError {
                    message: "root",
                    source: None,
                })),
            })),
        };

        assert_eq!(error_chain(&error), "middle: root");
    }
}
