use crate::server::{build_response, AdminResponse};
use crate::StaticRoute;

pub fn serve_static(route: &StaticRoute) -> AdminResponse {
    build_response(200, route.body.as_str().to_owned(), &route.content_type)
}
