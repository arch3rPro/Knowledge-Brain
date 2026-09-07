use kb_core::ErrorCode;
use kb_server::ServerPolicy;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn address(ip: [u8; 4]) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), 9432)
}

#[test]
fn loopback_is_read_only_by_default_and_sensitive_modes_require_auth() {
    let local = ServerPolicy::new(address([127, 0, 0, 1]), None, false).unwrap();
    assert!(!local.authentication_required());
    assert!(!local.allow_write());

    let lan = ServerPolicy::new(address([0, 0, 0, 0]), None, false).unwrap_err();
    assert_eq!(lan.code, ErrorCode::AuthDenied);

    let write = ServerPolicy::new(address([127, 0, 0, 1]), None, true).unwrap_err();
    assert_eq!(write.code, ErrorCode::AuthDenied);

    let authenticated =
        ServerPolicy::new(address([0, 0, 0, 0]), Some(" secret\n".into()), true).unwrap();
    assert!(authenticated.authentication_required());
    assert!(authenticated.allow_write());
    assert!(authenticated.authorize(Some("Bearer secret")));
    assert!(!authenticated.authorize(Some("Bearer wrong")));
    assert!(!authenticated.authorize(None));
}

#[test]
fn empty_or_header_unsafe_tokens_are_rejected() {
    for token in ["", "  \n", "has\nnewline", "Bearer nested"] {
        let error =
            ServerPolicy::new(address([127, 0, 0, 1]), Some(token.into()), false).unwrap_err();
        assert_eq!(error.code, ErrorCode::AuthDenied);
    }
}
