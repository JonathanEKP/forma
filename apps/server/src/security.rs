//! Browser response policy for the public application shell (plan §110.4).
//!
//! The browser will eventually hold a bearer credential, so the policy belongs
//! to the binary that serves the app and not only to one reverse proxy. That
//! keeps local and hosted behaviour identical and puts the header in reach of
//! the ordinary browser tests.

use actix_web::http::{header::HeaderValue, Uri};

/// Builds the exact policy shared by the editor and its API.
///
/// Supabase is the sole optional connection target. It is reduced to an origin
/// before interpolation so a path, query, or malformed header value cannot
/// silently widen or corrupt the policy.
pub fn content_security_policy(supabase_url: Option<&str>) -> Result<HeaderValue, String> {
    let extra_connection = supabase_url
        .map(supabase_origin)
        .transpose()?
        .map(|origin| format!(" {origin}"))
        .unwrap_or_default();

    let policy = format!(
        "default-src 'self'; \
         script-src 'self'; \
         style-src 'self'; \
         style-src-attr 'unsafe-inline'; \
         img-src 'self' data: blob:; \
         font-src 'self'; \
         connect-src 'self'{extra_connection}; \
         frame-ancestors 'none'; \
         base-uri 'none'; \
         object-src 'none';"
    );

    HeaderValue::from_str(&policy)
        .map_err(|error| format!("FORMA_SUPABASE_URL produced an invalid CSP header: {error}"))
}

fn supabase_origin(raw: &str) -> Result<String, String> {
    let uri = raw
        .parse::<Uri>()
        .map_err(|error| format!("FORMA_SUPABASE_URL is not a valid URL: {error}"))?;
    let scheme = uri
        .scheme_str()
        .filter(|scheme| matches!(*scheme, "http" | "https"))
        .ok_or_else(|| "FORMA_SUPABASE_URL must use http or https".to_string())?;
    let authority = uri
        .authority()
        .ok_or_else(|| "FORMA_SUPABASE_URL must include a host".to_string())?;

    Ok(format!("{scheme}://{authority}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_policy_has_no_external_connection_target() {
        let policy = content_security_policy(None).expect("the fixed policy is valid");
        assert_eq!(
            policy.to_str().expect("ASCII policy"),
            "default-src 'self'; script-src 'self'; style-src 'self'; \
             style-src-attr 'unsafe-inline'; img-src 'self' data: blob:; \
             font-src 'self'; connect-src 'self'; frame-ancestors 'none'; \
             base-uri 'none'; object-src 'none';"
        );
    }

    #[test]
    fn only_the_configured_supabase_origin_can_receive_connections() {
        let policy =
            content_security_policy(Some("https://project.supabase.co/auth/v1?ignored=path"))
                .expect("Supabase URL");

        assert!(policy
            .to_str()
            .expect("ASCII policy")
            .contains("connect-src 'self' https://project.supabase.co;"));
        assert!(!policy.to_str().expect("ASCII policy").contains("/auth/v1"));
    }

    #[test]
    fn malformed_or_non_http_urls_are_rejected() {
        assert!(content_security_policy(Some("not a url")).is_err());
        assert!(content_security_policy(Some("javascript:alert(1)")).is_err());
    }
}
