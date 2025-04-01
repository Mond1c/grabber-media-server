use std::{collections::HashMap, hash::Hash, sync::Arc};

use serde::{Deserialize, Serialize};
use webrtc::{api::{media_engine::MediaEngine, APIBuilder, API}, ice_transport::ice_server::RTCIceServer, interceptor::registry::Registry, peer_connection::{configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription, RTCPeerConnection}, rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType}, sdp::util::Codec, track::track_local::TrackLocal};


type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type TrackLocalBox = Arc<dyn TrackLocal + Send + Sync>;
type PeerConnectionBox = Arc<RTCPeerConnection>;

#[derive(Deserialize, Serialize, Clone, Debug)]
struct IceServer {
    urls: Vec<String>,
    username: Option<String>,
    credential: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct PeerConnectionConfig {
    ice_servers: Vec<IceServer>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct CodecConfig {
    mime_type: String,
    payload_type: u8,
    clock_rate: u32,
    channels: Option<u16>,
    std_fmtp_line: String,
}

impl CodecConfig {
    fn to_rtc_codec(&self) -> RTCRtpCodecParameters {
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: self.mime_type.clone(),
                clock_rate: self.clock_rate,
                channels: self.channels.unwrap_or(0),
                sdp_fmtp_line: self.std_fmtp_line.clone(),
                ..Default::default()
            },
            payload_type: self.payload_type,
            ..Default::default()
        }
    }
}
#[derive(Deserialize, Serialize)]
struct InitRequest {
    peer_connection_config: PeerConnectionConfig,
    codecs: Vec<CodecConfig>,
}

struct PeerManager {
    peer_connections: HashMap<String, PeerConnectionBox>,
    publisher_tracks: HashMap<String, Vec<TrackLocalBox>>,
    subscribers: HashMap<String, Vec<PeerConnectionBox>>,
    subscriber_to_publisher_key: HashMap<String, String>,
    setup_in_progress: HashMap<String, tokio::sync::oneshot::Sender<bool>>,
    rtc_config: RTCConfiguration,
    api_builder: Option<API>,
    codecs: Vec<CodecConfig>,
}

impl PeerManager {
    fn new() -> Self {
        PeerManager {
            peer_connections: HashMap::new(),
            publisher_tracks: HashMap::new(),
            subscribers: HashMap::new(),
            subscriber_to_publisher_key: HashMap::new(),
            setup_in_progress: HashMap::new(),
            rtc_config: RTCConfiguration::default(),
            api_builder: None,
            codecs: Vec::new(),
        }
    }

    fn initialize(&mut self, config: PeerConnectionConfig, codecs: Vec<CodecConfig>) -> Result<()> {
        let mut ice_servers = vec![];
        for server in config.ice_servers {
            ice_servers.push(RTCIceServer {
                urls: server.urls,
                username: server.username.unwrap_or_default(),
                credential: server.credential.unwrap_or_default(),
                ..Default::default()
            });
        }

        self.rtc_config = RTCConfiguration {
            ice_servers,
            ..Default::default()
        };

        self.codecs = codecs;

        let mut media_engine = MediaEngine::default();

        for codec in &self.codecs {
            let codec_type = if codec.mime_type.to_lowercase().contains("video") {
                RTPCodecType::Video
            } else {
                RTPCodecType::Audio
            };

            media_engine.register_codec(codec.to_rtc_codec(), codec_type)?;
        }

        let registry = Registry::new();
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();

        self.api_builder = Some(api);

        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        for (_, pc) in self.peer_connections.iter() {
            let _ = pc.close().await;
        }

        self.peer_connections.clear();
        self.publisher_tracks.clear();
        self.subscribers.clear();
        self.subscriber_to_publisher_key.clear();
        Ok(())
    }

    async fn delete_subcriber(&mut self, id: &str) -> Result<()> {
        if let Some(pc) = self.peer_connections.remove(id) {
            let _ = pc.close().await;
        }

        let publisher_key = if let Some(key) = self.subscriber_to_publisher_key.remove(id) {
            key
        } else {
            return Ok(());
        };

        if let Some(subs) = self.subscribers.get_mut(&publisher_key) {
            subs.retain(|sub| {
                let sub_id = sub.get_stats_id();
                sub_id != id
            });

            if subs.is_empty() {
                self.cleanup_publisher(&publisher_key).await?;
            }
        }

        Ok(())
    }

    async fn cleanup_publisher(&mut self, publisher_key: &str) -> Result<()> {
        if let Some(pc) = self.peer_connections.remove(publisher_key) {
            let _ = pc.close().await;
        }
        self.publisher_tracks.remove(publisher_key);
        self.subscribers.remove(publisher_key);
        Ok(())
    }


    async fn add_subscriber(
        &mut self,
        id: &str,
        publisher_socket_id: &str,
        stream_type: &str,
        offer: RTCSessionDescription,
    ) -> Result<RTCSessionDescription> {
        let publisher_key = format!("{}_{}", publisher_socket_id, stream_type);

        let tracks = if let Some(tracks) = self.publisher_tracks.get(&publisher_key) {
            if tracks.is_empty() {
                return Err("No tracks available".into())
            }
            tracks.clone()
        } else {
            return Err("Publisher not found".into());
        };

        let api = self.api_builder.as_ref().ok_or("API not init")?;
        let subscriber_pc = Arc::new(api.new_peer_connection(self.rtc_config.clone()).await?);


        let pc_clone = Arc::clone(&subscriber_pc);
        let id_clone = id.to_string();
        let publisher_key_clone = publisher_key.clone();

        Ok(offer)
    }
}

fn main() {
    println!("Hello, world!");
}
