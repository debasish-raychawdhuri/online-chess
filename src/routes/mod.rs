use actix_web::{web, HttpResponse, Responder};
use actix_files as fs;

/// HTTP handler for the index page
pub async fn index() -> impl Responder {
    HttpResponse::Ok().body("Chess Web App")
}

/// Configure the HTTP routes
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/ws").route(web::get().to(crate::websocket::ws_index))
    )
    .service(web::resource("/").route(web::get().to(index)))
    .service(fs::Files::new("/static", "./static").show_files_listing());
}
