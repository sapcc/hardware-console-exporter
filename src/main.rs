use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder, Result};
use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;
use std::sync::Mutex;

mod exporter;
mod settings;
use exporter::Exporter;
use settings::Settings;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct MethodLabels {
    pub method: Method,
}

pub struct Metrics {
    requests: Family<MethodLabels, Counter>,
}

impl Metrics {
    pub fn inc_requests(&self, method: Method) {
        self.requests.get_or_create(&MethodLabels { method }).inc();
    }
}

pub struct AppState {
    pub registry: Registry,
}

#[get("/metrics")]
pub async fn metrics_handler(
    state: web::Data<Mutex<AppState>>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let state = state.lock().unwrap();
    let mut body = String::new();
    let mut content: &str = "text/plain; version=1.0.0; charset=utf-8";
    encode(&mut body, &state.registry).unwrap();
    if let Some(content_type) = get_content_type(&req) {
        if content_type.contains("openmetrics-text") {
            content = "application/openmetrics-text; version=1.0.0; charset=utf-8";
        }
    }
    Ok(HttpResponse::Ok().content_type(content).body(body))
}

#[get("/handler")]
pub async fn some_handler(metrics: web::Data<Metrics>) -> impl Responder {
    metrics.inc_requests(Method::Get);
    "okay".to_string()
}

fn get_content_type<'a>(req: &'a HttpRequest) -> Option<&'a str> {
    req.headers().get("Accept")?.to_str().ok()
}
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let s = Settings::new().unwrap_or_else(|error| {
        panic!("error loading settings: {:?}", error);
    });

    let metrics = web::Data::new(Metrics {
        requests: Family::default(),
    });

    let mut state = AppState {
        registry: Registry::default(),
    };

    let mut exp = Exporter::new(s);
    state.registry.register(
        "hardware_consoles",
        "exporter for hardware consoles (dell, hpe, lenovo)",
        exp.metrics.clone(),
    );
    let state = web::Data::new(Mutex::new(state));
    actix_web::rt::spawn(async move { exp.run().await });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .app_data(metrics.clone())
            .service(metrics_handler)
            .service(some_handler)
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
