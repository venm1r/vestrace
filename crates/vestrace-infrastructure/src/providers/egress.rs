use vestrace_domain::DataDestination;

pub(super) fn destination_for_endpoint(
    endpoint: &reqwest::Url,
    redirects_disabled: bool,
    proxy_disabled: bool,
) -> DataDestination {
    use std::net::ToSocketAddrs;

    destination_for_endpoint_with_resolver(
        endpoint,
        redirects_disabled,
        proxy_disabled,
        |host, port| {
            (host, port)
                .to_socket_addrs()
                .map(|addresses| addresses.map(|address| address.ip()).collect())
        },
    )
}

pub(super) fn destination_for_endpoint_with_resolver<F>(
    endpoint: &reqwest::Url,
    redirects_disabled: bool,
    proxy_disabled: bool,
    resolve: F,
) -> DataDestination
where
    F: FnOnce(&str, u16) -> std::io::Result<Vec<std::net::IpAddr>>,
{
    let loopback = endpoint.host_str().is_some_and(|host| {
        let unbracketed = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host);
        match unbracketed.parse::<std::net::IpAddr>() {
            Ok(address) => address.is_loopback(),
            Err(_)
                if unbracketed.eq_ignore_ascii_case("localhost")
                    || unbracketed.eq_ignore_ascii_case("localhost.") =>
            {
                let Some(port) = endpoint.port_or_known_default() else {
                    return false;
                };
                // The name alone is not a locality proof: a hosts-file entry
                // can point `localhost` elsewhere. Refuse LocalModel when
                // resolution fails, yields nothing, or exposes even one
                // non-loopback route. DNS rebinding after this construction
                // check remains the separately stated non-goal in PLAN.md.
                resolve(unbracketed, port).is_ok_and(|addresses| {
                    !addresses.is_empty() && addresses.iter().all(|a| a.is_loopback())
                })
            }
            Err(_) => false,
        }
    });
    if loopback && redirects_disabled && proxy_disabled {
        DataDestination::LocalModel
    } else {
        DataDestination::RemoteProvider
    }
}
