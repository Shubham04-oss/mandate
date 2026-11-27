//! Error and panic reports to PostHog error tracking, as `$exception` events. Only static callsite
//! facts leave (module, file, line) with the trace and tenant ids: never the message or a field
//! value, which can hold database text or an email. Reporting never blocks or fails the caller.
use crate::events::posthog::Posthog;
use crate::ids::TenantId;
use opentelemetry::trace::{TraceContextExt, TraceId};
use serde_json::{Value, json};
use std::sync::OnceLock;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::dispatcher::WeakDispatch;
use tracing::field::{Field, Visit};
use tracing::span::{Id, Record};
use tracing::{Dispatch, Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;

/// Reports waiting to be sent; more are dropped, so an error storm can't grow memory.
const QUEUE: usize = 256;
/// Reports per `/batch/` call.
const BATCH: usize = 50;
/// The core as one PostHog "person" that never gets a profile.
const DISTINCT_ID: &str = "service:mandate-core";

/// The sending side of the report queue.
#[derive(Clone)]
pub struct Reporter(mpsc::Sender<Value>);

impl Reporter {
    /// A reporter and the queue it fills, holding at most `capacity` reports.
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<Value>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self(tx), rx)
    }

    /// Never waits: a full or closed queue drops the report.
    fn report(&self, exception: Value) {
        let _ = self.0.try_send(exception);
    }
}

/// The `$exception` event for code at `file:line`, named `kind` (a module path, or `panic`).
fn exception(
    kind: &str,
    file: &str,
    line: u32,
    handled: bool,
    trace: Option<TraceId>,
    tenant: Option<&TenantId>,
) -> Value {
    let at = format!("{file}:{line}");
    let mut props = json!({
        "$exception_list": [{
            "type": kind,
            "value": at,
            "mechanism": { "handled": handled, "synthetic": false },
            "stacktrace": { "type": "raw", "frames": [{
                "platform": "custom",
                "lang": "rust",
                "function": kind,
                "filename": file,
                "lineno": line,
                "in_app": true,
            }] },
        }],
        "$exception_level": "error",
        // One issue per callsite, however the message would have read.
        "$exception_fingerprint": format!("core:{at}"),
        "$process_person_profile": false,
        "$geoip_disable": true,
        "source": "core",
    });
    if let Some(trace) = trace {
        props["trace_id"] = trace.to_string().into();
        props["$trace_id"] = trace.to_string().into();
    }
    if let Some(tenant) = tenant {
        props["$groups"] = json!({ "organization": tenant.as_str() });
    }
    json!({
        "event": "$exception",
        "distinct_id": DISTINCT_ID,
        "uuid": Uuid::new_v4().to_string(),
        "timestamp": OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default(),
        "properties": props,
    })
}

/// A span's recorded `tenant.id`, kept so an error inside it can name its organization.
struct Tenant(TenantId);

/// Reads `tenant.id` from a record and nothing else; a value that isn't a valid id is dropped.
#[derive(Default)]
struct TenantField(Option<TenantId>);

impl Visit for TenantField {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "tenant.id" {
            self.0 = TenantId::parse(value).ok();
        }
    }

    fn record_debug(&mut self, _: &Field, _: &dyn std::fmt::Debug) {}
}

/// Reports every ERROR event; see the module docs for what leaves.
pub struct ExceptionLayer {
    reporter: Reporter,
    /// The subscriber this layer is part of, to read the OpenTelemetry layer's span context.
    dispatch: OnceLock<WeakDispatch>,
}

impl ExceptionLayer {
    pub fn new(reporter: Reporter) -> Self {
        Self {
            reporter,
            dispatch: OnceLock::new(),
        }
    }

    /// The trace of span `id`. `Span::current()` would see no subscriber here: tracing disables
    /// itself inside a subscriber callback, so the context is read through the stored dispatch.
    fn trace(&self, id: &Id) -> Option<TraceId> {
        let dispatch = self.dispatch.get()?.upgrade()?;
        let cx = tracing_opentelemetry::get_otel_context(id, &dispatch)?;
        let trace = cx.span().span_context().trace_id();
        (trace != TraceId::INVALID).then_some(trace)
    }
}

impl<S> Layer<S> for ExceptionLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_register_dispatch(&self, subscriber: &Dispatch) {
        let _ = self.dispatch.set(subscriber.downgrade());
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let mut tenant = TenantField::default();
        values.record(&mut tenant);
        if let (Some(tenant), Some(span)) = (tenant.0, ctx.span(id)) {
            span.extensions_mut().replace(Tenant(tenant));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        if *meta.level() != Level::ERROR {
            return;
        }
        let (trace, tenant) = match ctx.event_span(event) {
            Some(span) => (
                self.trace(&span.id()),
                span.scope()
                    .find_map(|s| s.extensions().get::<Tenant>().map(|t| t.0.clone())),
            ),
            None => (None, None),
        };
        self.reporter.report(exception(
            meta.target(),
            meta.file().unwrap_or("unknown"),
            meta.line().unwrap_or(0),
            true,
            trace,
            tenant.as_ref(),
        ));
    }
}

/// Reports a panic's location (never its payload), then runs the hook that was installed before.
pub fn report_panics(reporter: Reporter) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let (file, line) = info
            .location()
            .map_or(("unknown", 0), |l| (l.file(), l.line()));
        reporter.report(exception("panic", file, line, false, None, None));
        previous(info);
    }));
}

/// The background sender; `flush` sends what is queued and stops it.
pub struct Sender {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl Sender {
    /// Bounded by the PostHog client's timeout per batch, so shutdown can't hang on PostHog.
    pub async fn flush(self) {
        let _ = self.stop.send(());
        let _ = self.task.await;
    }
}

/// Starts reporting errors and panics to `posthog`: returns the layer for the subscriber and the
/// sender to flush at exit. Needs a Tokio runtime.
pub fn start(posthog: Posthog) -> (ExceptionLayer, Sender) {
    let (reporter, queue) = Reporter::new(QUEUE);
    report_panics(reporter.clone());
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(send(posthog, queue, stopped));
    (ExceptionLayer::new(reporter), Sender { stop, task })
}

/// Posts queued reports in batches until stopped (then drains the queue once) or the queue closes.
async fn send(posthog: Posthog, mut queue: mpsc::Receiver<Value>, mut stop: oneshot::Receiver<()>) {
    let mut batch = Vec::with_capacity(BATCH);
    loop {
        let last = tokio::select! {
            n = queue.recv_many(&mut batch, BATCH) => n == 0,
            _ = &mut stop => {
                while let Ok(e) = queue.try_recv() {
                    batch.push(e);
                }
                true
            }
        };
        if !batch.is_empty() {
            // WARN, not ERROR: a failed report must not report itself.
            if let Err(error) = posthog.post(&batch).await {
                tracing::warn!(%error, dropped = batch.len(), "exception reports dropped");
            }
            batch.clear();
        }
        if last {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::posthog::tests::fake;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing::subscriber::with_default;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    use tracing_subscriber::Registry;
    use tracing_subscriber::layer::SubscriberExt;

    /// Runs `f` under a subscriber with the OpenTelemetry span layer and an exception layer;
    /// returns what `f` returned and every report queued.
    fn reported<T>(f: impl FnOnce() -> T) -> (T, Vec<Value>) {
        let (reporter, mut queue) = Reporter::new(16);
        let provider = SdkTracerProvider::builder().build();
        let subscriber = Registry::default()
            .with(crate::telemetry::span_layer(provider.tracer("test")))
            .with(ExceptionLayer::new(reporter));
        let out = with_default(subscriber, f);
        let mut got = Vec::new();
        while let Ok(e) = queue.try_recv() {
            got.push(e);
        }
        (out, got)
    }

    #[test]
    fn an_error_reports_its_callsite_trace_and_tenant_and_nothing_it_said() {
        let ((trace, line), got) = reported(|| {
            let span = tracing::info_span!("request", tenant.id = tracing::field::Empty);
            let _g = span.enter();
            span.record("tenant.id", "acme");
            let line = line!() + 1;
            tracing::error!(
                user = "ana@example.com",
                "token sk-live-123 for {}",
                "ana@example.com"
            );
            let trace = span.context().span().span_context().trace_id();
            (trace, line)
        });
        let [e] = got.as_slice() else {
            panic!("one report, got {got:?}")
        };
        let all = e.to_string();
        assert!(!all.contains("ana@") && !all.contains("sk-live"), "{all}");
        assert_eq!(e["event"], "$exception");
        assert_eq!(e["distinct_id"], "service:mandate-core");
        assert!(Uuid::parse_str(e["uuid"].as_str().unwrap()).is_ok());
        assert!(OffsetDateTime::parse(e["timestamp"].as_str().unwrap(), &Rfc3339).is_ok());
        let p = &e["properties"];
        let at = format!("{}:{line}", file!());
        assert_eq!(
            p["$exception_list"],
            json!([{
                "type": module_path!(),
                "value": at,
                "mechanism": { "handled": true, "synthetic": false },
                "stacktrace": { "type": "raw", "frames": [{
                    "platform": "custom", "lang": "rust", "function": module_path!(),
                    "filename": file!(), "lineno": line, "in_app": true,
                }] },
            }])
        );
        assert_eq!(p["$exception_level"], "error");
        assert_eq!(p["$exception_fingerprint"], format!("core:{at}"));
        assert_eq!(p["$process_person_profile"], false);
        assert_eq!(p["$geoip_disable"], true);
        assert_eq!(p["source"], "core");
        assert_ne!(trace, TraceId::INVALID);
        assert_eq!(p["trace_id"], trace.to_string());
        assert_eq!(p["$trace_id"], trace.to_string());
        assert_eq!(p["$groups"], json!({ "organization": "acme" }));
    }

    #[test]
    fn a_nested_span_inherits_the_tenant_and_a_bad_one_is_dropped() {
        let (outer, got) = reported(|| {
            let outer = tracing::info_span!("request", tenant.id = tracing::field::Empty);
            let _o = outer.enter();
            outer.record("tenant.id", "acme");
            tracing::info_span!("inner").in_scope(|| tracing::error!("in a child span"));
            let odd = tracing::info_span!("odd", tenant.id = tracing::field::Empty);
            odd.record("tenant.id", "Ana Lee <ana@example.com>");
            // The invalid value is dropped and the request's tenant still applies.
            odd.in_scope(|| tracing::error!("under a bad tenant"));
            outer.context().span().span_context().trace_id()
        });
        assert_eq!(got.len(), 2);
        for e in &got {
            assert_eq!(e["properties"]["$groups"]["organization"], "acme");
            assert_eq!(e["properties"]["trace_id"], outer.to_string());
            assert!(!e.to_string().contains("ana@"));
        }
    }

    #[test]
    fn warnings_are_not_reported_and_an_error_outside_a_span_has_no_trace() {
        let ((), got) = reported(|| {
            tracing::warn!("odd");
            tracing::info!("routine");
            tracing::error!("no request");
        });
        let [e] = got.as_slice() else {
            panic!("one report, got {got:?}")
        };
        assert!(e["properties"].get("trace_id").is_none());
        assert!(e["properties"].get("$groups").is_none());
    }

    #[test]
    fn a_full_queue_drops_without_waiting() {
        let (reporter, mut queue) = Reporter::new(1);
        reporter.report(json!(1));
        reporter.report(json!(2));
        assert_eq!(queue.try_recv().unwrap(), json!(1));
        assert!(queue.try_recv().is_err());
        drop(queue);
        // A closed queue drops too.
        reporter.report(json!(3));
    }

    #[test]
    fn a_panic_reports_its_location_only() {
        let (reporter, mut queue) = Reporter::new(16);
        report_panics(reporter);
        let line = line!() + 1;
        let r = std::panic::catch_unwind(|| panic!("key sk-live-123 for ana@example.com"));
        assert!(r.is_err());
        let at = format!("{}:{line}", file!());
        // The hook is process-wide: other tests' panics may be queued too.
        let mut mine = None;
        while let Ok(e) = queue.try_recv() {
            if e["properties"]["$exception_list"][0]["value"] == at.as_str() {
                mine = Some(e);
            }
        }
        let e = mine.expect("the panic was reported");
        let all = e.to_string();
        assert!(!all.contains("sk-live") && !all.contains("ana@"), "{all}");
        let x = &e["properties"]["$exception_list"][0];
        assert_eq!(x["type"], "panic");
        assert_eq!(x["mechanism"]["handled"], false);
        assert_eq!(x["stacktrace"]["frames"][0]["lineno"], line);
    }

    #[tokio::test]
    async fn the_sender_posts_batches_and_drains_on_flush() {
        let (url, got) = fake(http::StatusCode::OK).await;
        let (reporter, queue) = Reporter::new(16);
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(send(
            Posthog::at(url, "phc_test".into()).unwrap(),
            queue,
            stopped,
        ));
        reporter.report(exception(
            "mandate::api",
            "src/api/mod.rs",
            7,
            true,
            None,
            None,
        ));
        reporter.report(exception("panic", "src/main.rs", 9, false, None, None));
        Sender { stop, task }.flush().await;
        let bodies = got.lock().unwrap().clone();
        let events: Vec<&Value> = bodies
            .iter()
            .flat_map(|b| b["batch"].as_array().unwrap())
            .collect();
        assert!(bodies.iter().all(|b| b["api_key"] == "phc_test"));
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e["event"] == "$exception"));
        assert_eq!(
            events[0]["properties"]["$exception_list"][0]["value"],
            "src/api/mod.rs:7"
        );
    }

    #[tokio::test]
    async fn a_failed_post_is_dropped_and_sending_goes_on() {
        let (url, got) = fake(http::StatusCode::SERVICE_UNAVAILABLE).await;
        let (reporter, queue) = Reporter::new(16);
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(send(Posthog::at(url, "k".into()).unwrap(), queue, stopped));
        reporter.report(exception("a", "a.rs", 1, true, None, None));
        // Let the first batch go (and fail) before the second is queued.
        while got.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        reporter.report(exception("b", "b.rs", 2, true, None, None));
        Sender { stop, task }.flush().await;
        assert_eq!(got.lock().unwrap().len(), 2);
    }
}
