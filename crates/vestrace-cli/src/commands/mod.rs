/// A connection string a reader can act on, with the credential removed.
///
/// A failure that names no database is hard to act on and one that names the
/// password is a leak, so the host survives and the credential does not. It
/// lives here because three commands report database failures and three copies
/// of this would drift apart.
pub(crate) fn redact_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(scheme_end) = url.find("://") {
            let scheme = &url[..scheme_end + 3];
            let host = &url[at_pos + 1..];
            return format!("{scheme}***@{host}");
        }
    }
    url.to_string()
}

pub mod conformance;
pub mod doctor;
pub mod mcp;
pub mod migrate;
pub mod operator;
pub mod rebuild;
pub mod recovery;
pub mod schema;
pub mod server;
pub mod worker;
