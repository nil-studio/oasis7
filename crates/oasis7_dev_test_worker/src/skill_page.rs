//! Public, secret-free instructions; serving these does not touch durable state.
use worker::*;
const SKILL: &str = include_str!("../static/SKILL.md");
const PAGE: &str = include_str!("../static/index.html");
pub(crate) fn serve(request: &Request, env: &Env) -> Result<Option<Response>> {
    let url = request.url()?;
    if url.query().is_some() || !matches!(request.method(), Method::Get | Method::Head) {
        return Ok(None);
    }
    let (body, content_type) = match url.path() {
        "/" | "/skill" => (PAGE, "text/html; charset=utf-8"),
        "/SKILL.md" => (SKILL, "text/markdown; charset=utf-8"),
        _ => return Ok(None),
    };
    // Closed values only: neither Host nor private policy is reflected into HTML.
    let environment = match env
        .var("ACCOUNT_SERVICE_ENVIRONMENT")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
    {
        Some("production") => "production",
        Some("test") => "test",
        _ => "legacy-test",
    };
    let mut response = Response::ok(if request.method() == Method::Head {
        String::new()
    } else {
        body.replace("{{ENVIRONMENT}}", environment)
    })?;
    let headers = response.headers_mut();
    headers.set("Content-Type", content_type)?;
    headers.set("Cache-Control", "no-store")?;
    headers.set("X-Content-Type-Options", "nosniff")?;
    headers.set("Referrer-Policy", "no-referrer")?;
    headers.set("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'")?;
    Ok(Some(response))
}
