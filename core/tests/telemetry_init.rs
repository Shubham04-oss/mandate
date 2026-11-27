//! `telemetry::init` installs the process-wide subscriber, so it gets a test binary of its own.
use mandate::exceptions::{ExceptionLayer, Reporter};

#[test]
fn an_error_reported_through_the_real_subscriber_carries_its_trace_id() {
    let (reporter, mut queue) = Reporter::new(16);
    let telemetry = mandate::telemetry::init(false, Some(ExceptionLayer::new(reporter)))
        .expect("the first subscriber installs");
    tracing::info_span!("request").in_scope(|| tracing::error!("boom"));
    let report = queue.try_recv().expect("the error was reported");
    let trace = report["properties"]["trace_id"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(trace.len(), 32, "no trace id: {report}");
    telemetry.shutdown();
}
