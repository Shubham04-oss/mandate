//! Logs and traces. Every request gets one span named by its route pattern, and every log line
//! inside it carries the span's `trace_id`. All logs go to stderr (CloudWatch); spans, and warn and
//! error logs, also go over OTLP to the collector when `OTEL_EXPORTER_OTLP_ENDPOINT` is set; errors
//! and panics also go to PostHog error tracking when `exceptions` is given (`crate::exceptions`).
//! Ids only: no bodies, headers, raw paths or text.
use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use http::HeaderMap;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_http::{HeaderExtractor, HeaderInjector};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use tracing::field::Empty;
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Identity, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

const SERVICE: &str = "mandate-core";
const ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("OTLP exporter: {0}")]
    Exporter(String),
    #[error("a tracing subscriber is already installed")]
    Installed,
}

/// Owns the providers; `shutdown` flushes what is still queued.
pub struct Telemetry {
    traces: SdkTracerProvider,
    logs: Option<SdkLoggerProvider>,
}

impl Telemetry {
    pub fn shutdown(self) {
        if let Err(error) = self.traces.shutdown() {
            tracing::warn!(%error, "flushing traces failed");
        }
        if let Some(Err(error)) = self.logs.map(|l| l.shutdown()) {
            tracing::warn!(%error, "flushing logs failed");
        }
    }
}

/// JSON logs on stderr (so `dev-assert` stdout carries the token and nothing else), plus trace
/// ids; with `export` and an endpoint configured, spans and warn+ logs also go to the collector.
pub fn init(
    export: bool,
    exceptions: Option<crate::exceptions::ExceptionLayer>,
) -> Result<Telemetry, TelemetryError> {
    let export = export && std::env::var_os(ENDPOINT).is_some();
    let traces = tracer_provider(export)?;
    let logs = export.then(logger_provider).transpose()?;
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(std::io::stderr),
        )
        .with(span_layer(traces.tracer(SERVICE)))
        .with(logs.as_ref().map(log_layer))
        // Never an Option: tracing-subscriber's Option<Layer> drops on_register_dispatch, and
        // without the dispatch the layer can't read a span's trace id.
        .with(match exceptions {
            Some(layer) => layer.boxed(),
            None => Identity::new().boxed(),
        })
        .try_init()
        .map_err(|_| TelemetryError::Installed)?;
    Ok(Telemetry { traces, logs })
}

fn resource() -> Resource {
    let service = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| SERVICE.into());
    Resource::builder().with_service_name(service).build()
}

/// Spans only: a log line would otherwise ride along as a span event, info text included. Warn and
/// error leave as logs (`log_layer`), joined to the trace by id.
pub(crate) fn span_layer<S>(tracer: opentelemetry_sdk::trace::Tracer) -> impl Layer<S> + use<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(tracing_subscriber::filter::filter_fn(|meta| meta.is_span()))
}

/// Warn and error only: info stays in CloudWatch, where it is free (owner's call, ORY-61).
fn log_layer<S>(logs: &SdkLoggerProvider) -> impl Layer<S> + use<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    OpenTelemetryTracingBridge::new(logs).with_filter(LevelFilter::WARN)
}

fn logger_provider() -> Result<SdkLoggerProvider, TelemetryError> {
    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .build()
        .map_err(|e| TelemetryError::Exporter(e.to_string()))?;
    Ok(SdkLoggerProvider::builder()
        .with_resource(resource())
        .with_batch_exporter(exporter)
        .build())
}

/// Without an exporter spans still get ids, so logs carry a `trace_id` either way.
fn tracer_provider(export: bool) -> Result<SdkTracerProvider, TelemetryError> {
    // Every span is kept here; the collector samples, so a caller's sampled flag decides nothing.
    let mut builder = SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .with_resource(resource());
    if export {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .build()
            .map_err(|e| TelemetryError::Exporter(e.to_string()))?;
        builder = builder.with_batch_exporter(exporter);
    }
    Ok(builder.build())
}

/// Only eve calls `/mcp`, over the private network (the tunnel serves `/v1` and `/healthz`), so its
/// `traceparent` may parent our span. Anything else came from the internet: it is linked, not
/// joined, so a client can't graft our spans onto a trace of its choosing.
fn trusts_parent(route: &str) -> bool {
    route == "/mcp" || route.starts_with("/mcp/")
}

/// The `traceparent`/`tracestate` of `headers`, if valid.
fn incoming(headers: &HeaderMap) -> Option<opentelemetry::Context> {
    let cx = TraceContextPropagator::new().extract(&HeaderExtractor(headers));
    cx.span().span_context().is_valid().then_some(cx)
}

/// Adds the current span's `traceparent` to an outgoing request's headers.
pub fn inject(headers: &mut HeaderMap) {
    TraceContextPropagator::new()
        .inject_context(&Span::current().context(), &mut HeaderInjector(headers));
}

/// Records the caller's tenant on the request span, once it is authenticated.
pub fn record_tenant(tenant: &str) {
    Span::current().record("tenant.id", tenant);
}

/// One span per request, named `METHOD /route/{pattern}`; never the raw path, which holds ids.
pub async fn http_span(req: Request, next: Next) -> Response {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", |p| p.as_str())
        .to_owned();
    // ECS probes it every few seconds: noise, not traffic.
    if route == "/healthz" {
        return next.run(req).await;
    }
    let method = req.method().clone();
    let span = tracing::info_span!(
        "request",
        otel.name = %format_args!("{method} {route}"),
        otel.kind = "server",
        otel.status_code = Empty,
        http.request.method = %method,
        http.route = %route,
        http.response.status_code = Empty,
        tenant.id = Empty,
        trace_id = Empty,
    );
    if let Some(cx) = incoming(req.headers()) {
        if trusts_parent(&route) {
            // Fails only once the span has started, which it hasn't.
            let _ = span.set_parent(cx);
        } else {
            span.add_link(cx.span().span_context().clone());
        }
    }
    let id = span.context().span().span_context().trace_id();
    if id != opentelemetry::trace::TraceId::INVALID {
        span.record("trace_id", tracing::field::display(id));
    }
    let res = next.run(req).instrument(span.clone()).await;
    let status = res.status();
    span.record("http.response.status_code", status.as_u16());
    if status.is_server_error() {
        span.record("otel.status_code", "error");
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::routing::get;
    use opentelemetry::trace::{SpanId, TraceId};
    use opentelemetry_sdk::trace::{InMemorySpanExporter, SpanData};
    use tower::ServiceExt;
    use tracing::subscriber::with_default;
    use tracing_subscriber::Registry;

    const PARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

    /// A router behind `http_span`, the spans it exported, and what the handler saw.
    fn run(path: &str, traceparent: Option<&str>) -> (Vec<SpanData>, http::StatusCode) {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber = Registry::default().with(span_layer(provider.tracer("test")));
        let app = Router::new()
            .route(
                "/v1/things/{id}",
                get(|| async {
                    record_tenant("acme");
                    tracing::info!("routine ann@example.com");
                    tracing::warn!("odd");
                    "ok"
                }),
            )
            // Mounted the way `mcp::router` mounts the MCP service.
            .nest_service("/mcp", get(|| async { "mcp" }))
            .route("/boom", get(|| async { http::StatusCode::BAD_GATEWAY }))
            .route("/healthz", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(http_span));
        let mut req = http::Request::get(path);
        if let Some(tp) = traceparent {
            req = req.header("traceparent", tp);
        }
        let req = req.body(Body::empty()).unwrap();
        let status = with_default(subscriber, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            rt.block_on(async { app.oneshot(req).await.unwrap().status() })
        });
        provider.force_flush().unwrap();
        (exporter.get_finished_spans().unwrap(), status)
    }

    fn attr(span: &SpanData, key: &str) -> Option<String> {
        span.attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.to_string())
    }

    #[test]
    fn the_span_is_named_by_route_pattern_never_the_raw_path() {
        let (spans, _) = run("/v1/things/secret-id-123?code=999999", None);
        let [span] = spans.as_slice() else {
            panic!("one span, got {spans:?}")
        };
        assert_eq!(span.name, "GET /v1/things/{id}");
        assert_eq!(attr(span, "http.route").as_deref(), Some("/v1/things/{id}"));
        assert_eq!(
            attr(span, "http.response.status_code").as_deref(),
            Some("200")
        );
        assert_eq!(attr(span, "tenant.id").as_deref(), Some("acme"));
        // Log lines are not span events: info stays in CloudWatch, warn+ goes out as a log.
        assert!(span.events.is_empty(), "{:?}", span.events);
        let all = format!("{span:?}");
        assert!(!all.contains("secret-id-123") && !all.contains("999999"));
        // The log field and the exported span agree on the trace.
        let logged = attr(span, "trace_id").unwrap();
        assert_eq!(logged, span.span_context.trace_id().to_string());
    }

    #[test]
    fn a_public_routes_traceparent_is_linked_not_joined() {
        let (spans, _) = run("/v1/things/1", Some(PARENT));
        let span = &spans[0];
        let theirs = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").unwrap();
        assert_ne!(span.span_context.trace_id(), theirs);
        assert_eq!(span.parent_span_id, SpanId::INVALID);
        assert_eq!(span.links.links.len(), 1);
        assert_eq!(span.links.links[0].span_context.trace_id(), theirs);
    }

    #[test]
    fn eves_traceparent_on_mcp_parents_the_span() {
        let (spans, _) = run("/mcp", Some(PARENT));
        let span = &spans[0];
        assert_eq!(
            span.span_context.trace_id(),
            TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").unwrap()
        );
        assert_eq!(
            span.parent_span_id,
            SpanId::from_hex("b7ad6b7169203331").unwrap()
        );
    }

    #[test]
    fn a_malformed_traceparent_is_ignored() {
        let (spans, _) = run("/mcp", Some("00-not-a-trace-01"));
        assert_eq!(spans[0].parent_span_id, SpanId::INVALID);
        assert!(spans[0].links.links.is_empty());
    }

    #[test]
    fn server_errors_mark_the_span_and_health_checks_make_none() {
        let (spans, status) = run("/boom", None);
        assert_eq!(status, http::StatusCode::BAD_GATEWAY);
        assert_eq!(spans[0].status, opentelemetry::trace::Status::error(""));
        let (spans, _) = run("/healthz", None);
        assert!(spans.is_empty());
    }

    #[test]
    fn only_warn_and_error_logs_are_exported_with_their_trace() {
        use opentelemetry_sdk::logs::InMemoryLogExporter;
        let exporter = InMemoryLogExporter::default();
        let logs = SdkLoggerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let traces = SdkTracerProvider::builder().build();
        let subscriber = Registry::default()
            .with(span_layer(traces.tracer("test")))
            .with(log_layer(&logs));
        let trace = with_default(subscriber, || {
            let span = tracing::info_span!("request");
            let _g = span.enter();
            tracing::info!("routine");
            tracing::warn!("odd");
            tracing::error!("broken");
            span.context().span().span_context().trace_id()
        });
        let got = exporter.get_emitted_logs().unwrap();
        let bodies: Vec<String> = got
            .iter()
            .map(|l| format!("{:?}", l.record.body()))
            .collect();
        assert_eq!(got.len(), 2, "{bodies:?}");
        assert!(bodies.iter().all(|b| !b.contains("routine")));
        for l in &got {
            assert_eq!(l.record.trace_context().unwrap().trace_id, trace);
        }
    }

    #[test]
    fn inject_writes_the_current_spans_traceparent() {
        let provider = SdkTracerProvider::builder().build();
        let subscriber = Registry::default().with(span_layer(provider.tracer("test")));
        with_default(subscriber, || {
            let span = tracing::info_span!("outer");
            let _g = span.enter();
            let mut headers = HeaderMap::new();
            inject(&mut headers);
            let tp = headers["traceparent"].to_str().unwrap();
            let id = span.context().span().span_context().trace_id().to_string();
            assert!(tp.starts_with(&format!("00-{id}-")), "{tp}");
        });
    }
}
