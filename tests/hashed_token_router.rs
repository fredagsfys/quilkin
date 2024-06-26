/*
 * Copyright 2024 Google LLC
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *       http://www.apache.org/licenses/LICENSE-2.0
 *
 *  Unless required by applicable law or agreed to in writing, software
 *  distributed under the License is distributed on an "AS IS" BASIS,
 *  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *  See the License for the specific language governing permissions and
 *  limitations under the License.
 */

use std::net::{Ipv6Addr, SocketAddr};

use tokio::time::{timeout, Duration};

use quilkin::{
    config::Filter,
    filters::{Capture, HashedTokenRouter, StaticFilter},
    net::endpoint::{metadata::MetadataView, Endpoint},
    test::{AddressType, TestHelper},
};

/// This test covers both hashed_token_router and capture filters,
/// since they work in concert together.
#[tokio::test]
async fn hashed_token_router() {
    let mut t = TestHelper::default();
    let mut echo = t.run_echo_server(AddressType::Ipv6).await;
    quilkin::test::map_to_localhost(&mut echo).await;

    let capture_yaml = "
suffix:
    size: 3
    remove: true
";
    let endpoint_metadata = "
quilkin.dev:
    tokens:
        - YWJj # abc
        ";

    let server_config = std::sync::Arc::new(quilkin::Config::default_non_agent());
    server_config.clusters.modify(|clusters| {
        clusters.insert_default(
            [Endpoint::with_metadata(
                echo.clone(),
                serde_yaml::from_str::<MetadataView<_>>(endpoint_metadata).unwrap(),
            )]
            .into(),
        );

        clusters.build_token_maps();
    });

    server_config.filters.store(
        quilkin::filters::FilterChain::try_create([
            Filter {
                name: Capture::factory().name().into(),
                label: None,
                config: serde_yaml::from_str(capture_yaml).unwrap(),
            },
            Filter {
                name: HashedTokenRouter::factory().name().into(),
                label: None,
                config: None,
            },
        ])
        .map(std::sync::Arc::new)
        .unwrap(),
    );

    let server_port = t.run_server(server_config, None, None).await;

    // valid packet
    let (mut recv_chan, socket) = t.open_socket_and_recv_multiple_packets().await;

    let local_addr = SocketAddr::from((Ipv6Addr::LOCALHOST, server_port));
    let msg = b"helloabc";
    tracing::trace!(%local_addr, "sending echo packet");
    socket.send_to(msg, &local_addr).await.unwrap();

    tracing::trace!("awaiting echo packet");
    assert_eq!(
        "hello",
        timeout(Duration::from_millis(500), recv_chan.recv())
            .await
            .expect("should have received a packet")
            .unwrap()
    );

    // send an invalid packet
    let msg = b"helloxyz";
    socket.send_to(msg, &local_addr).await.unwrap();

    let result = timeout(Duration::from_millis(500), recv_chan.recv()).await;
    assert!(result.is_err(), "should not have received a packet");
}
