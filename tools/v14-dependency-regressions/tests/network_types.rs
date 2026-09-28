use sc_network_types::{PeerId, multiaddr::Multiaddr};
#[test]
fn peer_id_json_round_trip_and_invalid_input() {
    let peer = PeerId::random();
    let encoded = serde_json::to_string(&peer).unwrap();
    assert_eq!(encoded, format!("\"{peer}\""));
    assert_eq!(serde_json::from_str::<PeerId>(&encoded).unwrap(), peer);
    assert!(serde_json::from_str::<PeerId>("\"not-a-peer\"").is_err());
}
#[test]
fn multiaddress_json_round_trip_and_invalid_input() {
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/30333".parse().unwrap();
    let encoded = serde_json::to_string(&address).unwrap();
    assert_eq!(encoded, "\"/ip4/127.0.0.1/tcp/30333\"");
    assert_eq!(serde_json::from_str::<Multiaddr>(&encoded).unwrap(), address);
    assert!(serde_json::from_str::<Multiaddr>("\"/invalid/protocol\"").is_err());
}
