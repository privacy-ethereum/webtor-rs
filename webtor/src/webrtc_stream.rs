//! WebRTC DataChannel stream for Snowflake transport
//!
//! This module provides WebRTC connectivity for the Snowflake client.
//! It uses the browser's native WebRTC API via web-sys bindings.
//!
//! Flow:
//! 1. Create RTCPeerConnection with STUN servers
//! 2. Create DataChannel (ordered, reliable)
//! 3. Generate SDP offer
//! 4. Exchange offer/answer via broker
//! 5. Set remote description
//! 6. Wait for DataChannel to open
//! 7. Use DataChannel for Turbo+KCP+SMUX transport

use crate::error::{Result, TorError};
use futures::{AsyncRead, AsyncWrite};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// STUN servers used for ICE gathering
/// These match the official Snowflake client configuration
pub const STUN_SERVERS: &[&str] = &[
    "stun:stun.l.google.com:19302",
    "stun:stun.antisip.com:3478",
    "stun:stun.bluesip.net:3478",
    "stun:stun.dus.net:3478",
    "stun:stun.epygi.com:3478",
    "stun:stun.sonetel.com:3478",
    "stun:stun.uls.co.za:3478",
    "stun:stun.voipgate.com:3478",
    "stun:stun.voys.nl:3478",
];

/// DataChannel configuration matching Snowflake Go client
pub const DATA_CHANNEL_LABEL: &str = "webrtc";

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use crate::snowflake_broker::BrokerClient;
    use futures::channel::mpsc;
    use futures::{FutureExt, StreamExt};
    use js_sys::{Array, Object, Reflect};
    use std::cell::RefCell;
    use std::rc::Rc;
    use tracing::{debug, info, trace, warn};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{
        RtcConfiguration, RtcDataChannel, RtcDataChannelInit, RtcDataChannelState,
        RtcIceGatheringState, RtcPeerConnection, RtcSdpType, RtcSessionDescriptionInit,
    };

    /// WebRTC stream wrapper for Snowflake
    pub struct WebRtcStream {
        #[allow(dead_code)]
        peer_connection: RtcPeerConnection,
        data_channel: RtcDataChannel,
        rx: mpsc::UnboundedReceiver<io::Result<Vec<u8>>>,
        buffer: Vec<u8>,
        // Keep closures alive
        #[allow(dead_code)]
        _on_message: Closure<dyn FnMut(web_sys::MessageEvent)>,
        #[allow(dead_code)]
        _on_error: Closure<dyn FnMut(web_sys::Event)>,
        #[allow(dead_code)]
        _on_close: Closure<dyn FnMut(web_sys::Event)>,
    }

    impl WebRtcStream {
        /// Connect to a Snowflake proxy via the broker
        pub async fn connect(broker_url: &str, fingerprint: &str) -> Result<Self> {
            info!("Creating WebRTC connection for Snowflake");

            // 1. Create RTCPeerConnection with STUN servers
            let config = create_rtc_config()?;
            let pc = RtcPeerConnection::new_with_configuration(&config)
                .map_err(|e| TorError::Network(format!("Failed to create RTCPeerConnection: {:?}", e)))?;

            debug!("RTCPeerConnection created");

            // 2. Create DataChannel (must be before creating offer)
            let dc_init = RtcDataChannelInit::new();
            // ordered and reliable by default (like TCP)
            let dc = pc.create_data_channel_with_data_channel_dict(DATA_CHANNEL_LABEL, &dc_init);

            debug!("DataChannel created: {}", DATA_CHANNEL_LABEL);

            // 3. Setup channel for receiving messages
            let (tx, rx) = mpsc::unbounded();
            let tx_msg = tx.clone();
            let tx_err = tx.clone();
            let tx_close = tx;

            // Set binary type
            dc.set_binary_type(web_sys::RtcDataChannelType::Arraybuffer);

            // onmessage handler
            let on_message = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&abuf);
                    let data = array.to_vec();
                    trace!("WebRTC received {} bytes", data.len());
                    let _ = tx_msg.unbounded_send(Ok(data));
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>);
            dc.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            // onerror handler
            let on_error = Closure::wrap(Box::new(move |_e: web_sys::Event| {
                warn!("WebRTC DataChannel error");
                let _ = tx_err.unbounded_send(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "DataChannel error",
                )));
            }) as Box<dyn FnMut(web_sys::Event)>);
            dc.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            // onclose handler
            let on_close = Closure::wrap(Box::new(move |_e: web_sys::Event| {
                debug!("WebRTC DataChannel closed");
                // Send EOF by closing the channel
                tx_close.close_channel();
            }) as Box<dyn FnMut(web_sys::Event)>);
            dc.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            // 4. Wait for ICE gathering to complete
            let offer_sdp = create_and_gather_offer(&pc).await?;
            info!("SDP offer created ({} bytes)", offer_sdp.len());

            // 5. Exchange offer/answer via broker
            let broker = BrokerClient::new(broker_url)
                .with_fingerprint(fingerprint.to_string());
            let answer_json = broker.negotiate(&offer_sdp).await?;
            info!("Got SDP answer from broker");

            // 6. Parse and set remote description
            // Answer is JSON: {"type":"answer","sdp":"..."}
            let answer_sdp = parse_sdp_answer(&answer_json)?;
            let answer_init = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
            answer_init.set_sdp(&answer_sdp);
            
            let set_remote = pc.set_remote_description(&answer_init);
            wasm_bindgen_futures::JsFuture::from(set_remote)
                .await
                .map_err(|e| TorError::Network(format!("Failed to set remote description: {:?}", e)))?;

            debug!("Remote description set");

            // 7. Wait for DataChannel to open
            wait_for_channel_open(&dc).await?;
            info!("WebRTC DataChannel opened!");

            Ok(Self {
                peer_connection: pc,
                data_channel: dc,
                rx,
                buffer: Vec::new(),
                _on_message: on_message,
                _on_error: on_error,
                _on_close: on_close,
            })
        }

        /// Send data over the DataChannel
        pub fn send(&self, data: &[u8]) -> Result<()> {
            if self.data_channel.ready_state() != RtcDataChannelState::Open {
                return Err(TorError::Network("DataChannel not open".to_string()));
            }

            self.data_channel
                .send_with_u8_array(data)
                .map_err(|e| TorError::Network(format!("Failed to send: {:?}", e)))
        }
    }

    /// Create RTCConfiguration with STUN servers
    fn create_rtc_config() -> Result<RtcConfiguration> {
        let config = RtcConfiguration::new();
        
        let ice_servers = Array::new();
        for stun_url in STUN_SERVERS {
            let server = Object::new();
            let urls = Array::new();
            urls.push(&JsValue::from_str(stun_url));
            Reflect::set(&server, &JsValue::from_str("urls"), &urls)
                .map_err(|_| TorError::Internal("Failed to set STUN URL".to_string()))?;
            ice_servers.push(&server);
        }
        
        config.set_ice_servers(&ice_servers);
        Ok(config)
    }

    /// Create SDP offer and wait for ICE gathering to complete
    async fn create_and_gather_offer(pc: &RtcPeerConnection) -> Result<String> {
        // Create offer
        let offer = wasm_bindgen_futures::JsFuture::from(pc.create_offer())
            .await
            .map_err(|e| TorError::Network(format!("Failed to create offer: {:?}", e)))?;
        
        let offer_init: RtcSessionDescriptionInit = offer.unchecked_into();
        
        // Set local description
        wasm_bindgen_futures::JsFuture::from(pc.set_local_description(&offer_init))
            .await
            .map_err(|e| TorError::Network(format!("Failed to set local description: {:?}", e)))?;

        // Wait for ICE gathering to complete
        info!("ICE gathering state: {:?}", pc.ice_gathering_state());
        if pc.ice_gathering_state() != RtcIceGatheringState::Complete {
            info!("Waiting for ICE gathering to complete...");
            wait_for_ice_gathering(pc).await?;
            info!("ICE gathering finished, state: {:?}", pc.ice_gathering_state());
        }

        // Get the complete SDP with ICE candidates
        let local_desc = pc.local_description()
            .ok_or_else(|| TorError::Internal("No local description after gathering".to_string()))?;
        
        let sdp = local_desc.sdp();
        
        // Log SDP details for debugging
        let ice_candidate_count = sdp.matches("a=candidate:").count();
        info!("SDP contains {} ICE candidates, {} bytes total", ice_candidate_count, sdp.len());
        
        if ice_candidate_count == 0 {
            warn!("SDP has no ICE candidates - this may cause broker matching to fail");
        }
        
        // Serialize as JSON object like Go client does: {"type":"offer","sdp":"..."}
        let offer_json = serde_json::json!({
            "type": "offer",
            "sdp": sdp
        });
        let serialized = serde_json::to_string(&offer_json)
            .map_err(|e| TorError::Internal(format!("Failed to serialize SDP offer: {}", e)))?;
        
        debug!("Serialized SDP offer: {} bytes", serialized.len());
        Ok(serialized)
    }

    /// Parse SDP answer from JSON format {"type":"answer","sdp":"..."}
    fn parse_sdp_answer(json_str: &str) -> Result<String> {
        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| TorError::Protocol(format!("Failed to parse SDP answer JSON: {}", e)))?;
        
        let sdp = parsed.get("sdp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| TorError::Protocol("SDP answer missing 'sdp' field".to_string()))?;
        
        Ok(sdp.to_string())
    }

    /// Wait for ICE gathering state to become complete
    async fn wait_for_ice_gathering(pc: &RtcPeerConnection) -> Result<()> {
        let (tx, rx) = futures::channel::oneshot::channel::<()>();
        let tx = Rc::new(RefCell::new(Some(tx)));

        // Clone pc reference for use in closure
        let pc_clone = pc.clone();
        let tx_clone = tx.clone();
        let on_ice_gathering = Closure::wrap(Box::new(move |_: web_sys::Event| {
            // Only signal when gathering is actually complete
            if pc_clone.ice_gathering_state() == RtcIceGatheringState::Complete {
                info!("ICE gathering completed");
                if let Some(tx) = tx_clone.borrow_mut().take() {
                    let _ = tx.send(());
                }
            } else {
                info!("ICE gathering state changed to: {:?}", pc_clone.ice_gathering_state());
            }
        }) as Box<dyn FnMut(web_sys::Event)>);

        pc.set_onicegatheringstatechange(Some(on_ice_gathering.as_ref().unchecked_ref()));

        // Also check current state in case we missed the transition
        if pc.ice_gathering_state() == RtcIceGatheringState::Complete {
            info!("ICE gathering already complete");
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(());
            }
        }

        // Wait with timeout
        let timeout = gloo_timers::future::TimeoutFuture::new(10_000);
        futures::select! {
            _ = rx.fuse() => {
                info!("ICE gathering finished successfully");
            }
            _ = timeout.fuse() => {
                warn!("ICE gathering timeout after 10s - proceeding with partial candidates");
            }
        }

        pc.set_onicegatheringstatechange(None);
        Ok(())
    }

    /// Wait for DataChannel to open
    async fn wait_for_channel_open(dc: &RtcDataChannel) -> Result<()> {
        if dc.ready_state() == RtcDataChannelState::Open {
            return Ok(());
        }

        let (tx, rx) = futures::channel::oneshot::channel::<std::result::Result<(), String>>();
        let tx = Rc::new(RefCell::new(Some(tx)));

        let tx_open = tx.clone();
        let on_open = Closure::wrap(Box::new(move |_: web_sys::Event| {
            if let Some(tx) = tx_open.borrow_mut().take() {
                let _ = tx.send(Ok(()));
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        dc.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let tx_err = tx.clone();
        let _on_error_open = Closure::wrap(Box::new(move |_: web_sys::Event| {
            if let Some(tx) = tx_err.borrow_mut().take() {
                let _ = tx.send(Err("DataChannel error during open".to_string()));
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        // Note: We already have onerror set, but this is for the opening phase

        // Wait with timeout
        let timeout = gloo_timers::future::TimeoutFuture::new(30_000);

        let result = futures::select! {
            r = rx.fuse() => r.map_err(|_| TorError::Network("Channel open cancelled".to_string()))?,
            _ = timeout.fuse() => Err("DataChannel open timeout".to_string()),
        };

        // CRITICAL: Clear the onopen handler before returning to prevent
        // "closure invoked after being dropped" errors
        dc.set_onopen(None);

        result.map_err(TorError::Network)
    }

    impl Drop for WebRtcStream {
        fn drop(&mut self) {
            // Clear handlers BEFORE drop to prevent "closure invoked after being dropped" errors
            self.data_channel.set_onmessage(None);
            self.data_channel.set_onerror(None);
            self.data_channel.set_onclose(None);
            self.data_channel.set_onopen(None);
            // Also close the connection
            self.data_channel.close();
            self.peer_connection.close();
        }
    }

    impl AsyncRead for WebRtcStream {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            // Drain internal buffer first
            if !self.buffer.is_empty() {
                let len = std::cmp::min(buf.len(), self.buffer.len());
                buf[..len].copy_from_slice(&self.buffer[..len]);
                self.buffer.drain(..len);
                return Poll::Ready(Ok(len));
            }

            // Poll for new messages
            match self.rx.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(data))) => {
                    if data.is_empty() {
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                    let len = std::cmp::min(buf.len(), data.len());
                    buf[..len].copy_from_slice(&data[..len]);
                    if len < data.len() {
                        self.buffer.extend_from_slice(&data[len..]);
                    }
                    Poll::Ready(Ok(len))
                }
                Poll::Ready(Some(Err(e))) => Poll::Ready(Err(e)),
                Poll::Ready(None) => Poll::Ready(Ok(0)), // EOF
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl AsyncWrite for WebRtcStream {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            match self.send(buf) {
                Ok(()) => Poll::Ready(Ok(buf.len())),
                Err(e) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e.to_string()))),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.data_channel.close();
            Poll::Ready(Ok(()))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;

    /// Native WebRTC stream (stub for now - would use webrtc-rs crate)
    pub struct WebRtcStream {
        _placeholder: (),
    }

    impl WebRtcStream {
        pub async fn connect(_broker_url: &str, _fingerprint: &str) -> Result<Self> {
            // For native, we would use the webrtc-rs crate
            // For now, return an error since native Snowflake is less common
            Err(TorError::Internal(
                "Native WebRTC not yet implemented - use WebTunnel bridge instead".to_string(),
            ))
        }
    }

    impl AsyncRead for WebRtcStream {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Native WebRTC not implemented",
            )))
        }
    }

    impl AsyncWrite for WebRtcStream {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Native WebRTC not implemented",
            )))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Native WebRTC not implemented",
            )))
        }

        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Native WebRTC not implemented",
            )))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
