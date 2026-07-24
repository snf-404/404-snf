// SPDX-License-Identifier: Apache-2.0

//! BlueZ backend (`bluer`). Linux-only.
//!
//! Registers the SNF Telemetry GATT peripheral (`PROTOCOL.md` §3), advertises
//! `404-SNF`, notifies telemetry, and forwards Stream Control writes to the
//! application. It is a thin transport: all byte layout lives in
//! [`crate::protocol`], all framing in [`crate::fragment`]. Because `bluer`
//! talks to `bluetoothd` over D-Bus, this module compiles and runs only on
//! Linux; the portable core is exercised by `cargo test` on the dev host.
//!
//! ## Notify model
//!
//! Each notify characteristic owns a [`broadcast`] channel. When a central
//! subscribes, BlueZ invokes the characteristic's notify closure with a
//! [`CharacteristicNotifier`]; the closure subscribes to the broadcast and
//! forwards each frame. [`publish`](BluezPeripheral::publish) fragments a
//! message and pushes the frames into the matching channel. A lagging subscriber
//! drops the oldest frames rather than blocking the pipeline, which matches the
//! backpressure rule in `PROTOCOL.md` §13 (never let stale pose/point-cloud
//! frames queue). BlueZ's `Fun` notify callback exposes no MTU, so the negotiated
//! link MTU is sampled from the read and write requests (Protocol Info read,
//! status/vitals/fatigue reads, Control writes) and cached for the fragmenter.
//!
//! ## Security
//!
//! In production (`ENCRYPTION_REQUIRED` capability set) the peripheral registers
//! a pairing agent and opens a bounded pairing window at startup, so first
//! pairing is not permanent Just-Works, and marks the Read/Write access points
//! `encrypt-*` so an unpaired central can neither read telemetry nor write
//! Control (`PROTOCOL.md` §15). A development build clears the capability bit and
//! runs without pairing.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use bluer::{
    adv::Advertisement,
    adv::AdvertisementHandle,
    agent::{Agent, AgentHandle, ReqError, RequestAuthorization, RequestConfirmation},
    gatt::local::{
        Application, ApplicationHandle, Characteristic, CharacteristicNotifier,
        CharacteristicNotify, CharacteristicNotifyMethod, CharacteristicRead, CharacteristicWrite,
        CharacteristicWriteMethod, Service,
    },
};
use futures::FutureExt;
use tokio::sync::{Mutex, broadcast, mpsc};
use uuid::Uuid;

use crate::backend::{BleError, BleTransport, Telemetry};
use crate::fragment::{fragment, frame_unfragmented};
use crate::protocol::{
    ControlRequest, ControlResponse, DEVICE_STATUS_UUID, FATIGUE_UUID, MessageType,
    POINT_CLOUD_UUID, POSE_UUID, PROTOCOL_INFO_UUID, ProtocolInfo, SERVICE_UUID,
    STREAM_CONTROL_UUID, VITALS_UUID, capabilities,
};

/// Depth of each characteristic's frame channel. Small: the protocol drops old
/// real-time frames rather than buffering them (`PROTOCOL.md` §13).
const CHANNEL_DEPTH: usize = 16;

/// Fallback ATT MTU until a subscription reports the negotiated value. `23` is
/// the guaranteed BLE minimum, so frames sized against it fit any link.
const DEFAULT_ATT_MTU: usize = 23;

/// How long the peripheral stays pairable after `start()` when encryption is
/// required (`PROTOCOL.md` §15: a bounded, one-time pairing window rather than
/// permanent Just-Works pairability). Bonded centrals reconnect with stored keys
/// after it closes; new bonds are refused.
const PAIRING_WINDOW_SECS: u32 = 120;

/// A framed notification value ready to hand to `bluer`. Shared so it can fan
/// out to multiple subscribers without cloning the bytes per receiver.
type Frame = Arc<Vec<u8>>;

/// Broadcast senders, one per notify characteristic.
#[derive(Clone)]
struct Channels {
    status: broadcast::Sender<Frame>,
    vitals: broadcast::Sender<Frame>,
    fatigue: broadcast::Sender<Frame>,
    pose: broadcast::Sender<Frame>,
    point_cloud: broadcast::Sender<Frame>,
    /// Indicated Control Responses.
    control: broadcast::Sender<Frame>,
}

impl Channels {
    fn new() -> Self {
        Self {
            status: broadcast::channel(CHANNEL_DEPTH).0,
            vitals: broadcast::channel(CHANNEL_DEPTH).0,
            fatigue: broadcast::channel(CHANNEL_DEPTH).0,
            pose: broadcast::channel(CHANNEL_DEPTH).0,
            point_cloud: broadcast::channel(CHANNEL_DEPTH).0,
            control: broadcast::channel(CHANNEL_DEPTH).0,
        }
    }

    fn for_telemetry(&self, telemetry: &Telemetry) -> &broadcast::Sender<Frame> {
        match telemetry {
            Telemetry::Status(_) => &self.status,
            Telemetry::Vitals(_) => &self.vitals,
            Telemetry::Fatigue(_) => &self.fatigue,
            Telemetry::Pose(_) => &self.pose,
            Telemetry::PointCloud(_) => &self.point_cloud,
        }
    }
}

/// Per-message-type monotonic sequence counters (`PROTOCOL.md` §6: each message
/// type increments independently; wrap is modular).
#[derive(Default)]
struct SeqCounters {
    status: u32,
    vitals: u32,
    fatigue: u32,
    pose: u32,
    point_cloud: u32,
    control: u32,
}

impl SeqCounters {
    fn next(&mut self, message_type: MessageType) -> u32 {
        let slot = match message_type {
            MessageType::DeviceStatus => &mut self.status,
            MessageType::Vitals => &mut self.vitals,
            MessageType::Fatigue => &mut self.fatigue,
            MessageType::Pose => &mut self.pose,
            MessageType::PointCloud => &mut self.point_cloud,
            MessageType::ControlResponse => &mut self.control,
        };
        let value = *slot;
        *slot = slot.wrapping_add(1);
        value
    }
}

/// BlueZ-backed SNF telemetry peripheral.
pub struct BluezPeripheral {
    adapter_name: Option<String>,
    info: ProtocolInfo,

    channels: Channels,
    /// Latest framed (header + payload) value for each Read+Notify telemetry
    /// characteristic, served to a `Read` before the first notify (`PROTOCOL.md`
    /// §3, §11). Shared with the read closures.
    latest: ReadCaches,
    /// Negotiated ATT MTU, sampled from read/write requests (BlueZ's `Fun` notify
    /// callback exposes none).
    mtu: Arc<AtomicUsize>,
    /// Whether the one-time pairing window is open. The agent accepts a new bond
    /// only while this is `true` (`PROTOCOL.md` §15); [`start`](Self::start)
    /// opens it for [`PAIRING_WINDOW_SECS`].
    pairing_open: Arc<AtomicBool>,

    seq: SeqCounters,
    boot: std::time::Instant,

    /// Receiver of parsed Stream Control requests; taken by the application via
    /// [`control_requests`](BleTransport::control_requests).
    control_rx: Option<mpsc::Receiver<ControlRequest>>,
    control_tx: mpsc::Sender<ControlRequest>,

    // Kept alive for the peripheral's lifetime; dropping unregisters them.
    _app: Option<ApplicationHandle>,
    _adv: Option<AdvertisementHandle>,
    _agent: Option<AgentHandle>,
    _session: Option<bluer::Session>,
}

/// Cached framed values for the three Read+Notify telemetry characteristics
/// (Device Status, Vitals, Fatigue — `PROTOCOL.md` §3). A connecting central's
/// `Read` returns the same header+payload it would receive as a notification.
#[derive(Clone)]
struct ReadCaches {
    status: Arc<Mutex<Vec<u8>>>,
    vitals: Arc<Mutex<Vec<u8>>>,
    fatigue: Arc<Mutex<Vec<u8>>>,
}

impl ReadCaches {
    fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(Vec::new())),
            vitals: Arc::new(Mutex::new(Vec::new())),
            fatigue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The cache backing `telemetry`'s Read, if it has one (Pose and Point Cloud
    /// are notify-only and return `None`).
    fn for_telemetry(&self, telemetry: &Telemetry) -> Option<&Arc<Mutex<Vec<u8>>>> {
        match telemetry {
            Telemetry::Status(_) => Some(&self.status),
            Telemetry::Vitals(_) => Some(&self.vitals),
            Telemetry::Fatigue(_) => Some(&self.fatigue),
            Telemetry::Pose(_) | Telemetry::PointCloud(_) => None,
        }
    }
}

impl BluezPeripheral {
    /// Create a peripheral, optionally pinned to a specific adapter (e.g.
    /// `hci0`; `None` uses the default adapter). `info` is the static Protocol
    /// Info served on connect — its capability bits must reflect which streams
    /// this build actually produces (`PROTOCOL.md` §2, §9).
    pub fn new(adapter_name: Option<String>, info: ProtocolInfo) -> Self {
        let (control_tx, control_rx) = mpsc::channel(32);
        Self {
            adapter_name,
            info,
            channels: Channels::new(),
            latest: ReadCaches::new(),
            mtu: Arc::new(AtomicUsize::new(DEFAULT_ATT_MTU)),
            pairing_open: Arc::new(AtomicBool::new(false)),
            seq: SeqCounters::default(),
            boot: std::time::Instant::now(),
            control_rx: Some(control_rx),
            control_tx,
            _app: None,
            _adv: None,
            _agent: None,
            _session: None,
        }
    }

    /// Whether this build enforces encrypted, paired access. Driven by the
    /// advertised `ENCRYPTION_REQUIRED` capability so Protocol Info and the GATT
    /// security flags never disagree (`PROTOCOL.md` §5, §15). A development build
    /// clears the bit to allow unauthenticated access.
    fn require_encryption(&self) -> bool {
        self.info.capabilities & capabilities::ENCRYPTION_REQUIRED != 0
    }

    /// Close the one-time pairing window early (e.g. once a bond exists). The
    /// agent then refuses further new bonds even if the adapter is still
    /// pairable (`PROTOCOL.md` §15).
    pub fn close_pairing_window(&self) {
        self.pairing_open.store(false, Ordering::Relaxed);
    }

    /// Milliseconds since construction, wrapping at `u32::MAX` per the header's
    /// `timestamp_ms` convention (`PROTOCOL.md` §6).
    fn timestamp_ms(&self) -> u32 {
        self.boot.elapsed().as_millis() as u32
    }

    /// Fragment `payload` for the current link and push the frames into `sender`.
    /// A send error means no central is subscribed, which is not fatal.
    fn dispatch(
        &mut self,
        message_type: MessageType,
        flags: u8,
        payload: &[u8],
        sender: &broadcast::Sender<Frame>,
    ) -> Result<(u32, u32), BleError> {
        let mtu = self.mtu.load(Ordering::Relaxed).max(DEFAULT_ATT_MTU);
        let seq = self.seq.next(message_type);
        let timestamp = self.timestamp_ms();
        let frames = fragment(message_type, seq, timestamp, flags, payload, mtu)
            .map_err(|_| BleError::LinkTooConstrained)?;
        for frame in frames {
            let _ = sender.send(Arc::new(frame));
        }
        Ok((seq, timestamp))
    }

    /// Build the GATT application: the primary service plus its seven
    /// characteristics, wired to the broadcast channels and the control sink.
    ///
    /// When [`require_encryption`](Self::require_encryption) is set, the Read and
    /// Write access points carry BlueZ `encrypt-*` flags so an unpaired central
    /// cannot read telemetry or write Control (`PROTOCOL.md` §15). Protocol Info
    /// stays cleartext so a client can discover the `ENCRYPTION_REQUIRED`
    /// capability before pairing (§5, §14). Pose and Point Cloud are notify-only
    /// — BlueZ has no encrypt-notify flag — but they cannot be enabled without an
    /// encrypted Control write, so they inherit the same gate.
    fn build_application(&self) -> Application {
        let info_bytes = self.info.encode();
        let secure = self.require_encryption();

        Application {
            services: vec![Service {
                uuid: SERVICE_UUID,
                primary: true,
                characteristics: vec![
                    read_characteristic(PROTOCOL_INFO_UUID, info_bytes, self.mtu.clone()),
                    control_characteristic(
                        STREAM_CONTROL_UUID,
                        self.channels.control.clone(),
                        self.control_tx.clone(),
                        self.mtu.clone(),
                        secure,
                    ),
                    readable_notify_characteristic(
                        DEVICE_STATUS_UUID,
                        self.channels.status.clone(),
                        self.latest.status.clone(),
                        self.mtu.clone(),
                        secure,
                    ),
                    readable_notify_characteristic(
                        VITALS_UUID,
                        self.channels.vitals.clone(),
                        self.latest.vitals.clone(),
                        self.mtu.clone(),
                        secure,
                    ),
                    readable_notify_characteristic(
                        FATIGUE_UUID,
                        self.channels.fatigue.clone(),
                        self.latest.fatigue.clone(),
                        self.mtu.clone(),
                        secure,
                    ),
                    notify_characteristic(POSE_UUID, self.channels.pose.clone()),
                    notify_characteristic(POINT_CLOUD_UUID, self.channels.point_cloud.clone()),
                ],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// Register the pairing agent and open the bounded pairing window
    /// (`PROTOCOL.md` §15). The agent accepts a bond only while `pairing_open`
    /// is set; `set_pairable_timeout` closes the window at the adapter level too,
    /// so the peripheral is never permanently Just-Works pairable.
    async fn arm_pairing(
        &mut self,
        session: &bluer::Session,
        adapter: &bluer::Adapter,
    ) -> Result<(), BleError> {
        self.pairing_open.store(true, Ordering::Relaxed);
        let confirm_gate = self.pairing_open.clone();
        let authorize_gate = self.pairing_open.clone();

        let agent = Agent {
            request_default: true,
            // Numeric-comparison confirmation (LE Secure Connections). We have no
            // display, so within the window we accept; outside it we reject.
            request_confirmation: Some(Box::new(move |_req: RequestConfirmation| {
                let open = confirm_gate.load(Ordering::Relaxed);
                async move {
                    if open {
                        Ok(())
                    } else {
                        Err(ReqError::Rejected)
                    }
                }
                .boxed()
            })),
            // Just-Works incoming pairing: only authorize inside the window.
            request_authorization: Some(Box::new(move |_req: RequestAuthorization| {
                let open = authorize_gate.load(Ordering::Relaxed);
                async move {
                    if open {
                        Ok(())
                    } else {
                        Err(ReqError::Rejected)
                    }
                }
                .boxed()
            })),
            ..Default::default()
        };

        let agent_handle = session
            .register_agent(agent)
            .await
            .map_err(|e| BleError::Backend(e.to_string()))?;
        self._agent = Some(agent_handle);

        adapter
            .set_pairable_timeout(PAIRING_WINDOW_SECS)
            .await
            .map_err(|e| BleError::Backend(e.to_string()))?;
        adapter
            .set_pairable(true)
            .await
            .map_err(|e| BleError::Backend(e.to_string()))?;
        Ok(())
    }
}

impl BleTransport for BluezPeripheral {
    async fn start(&mut self) -> Result<(), BleError> {
        let session = bluer::Session::new()
            .await
            .map_err(|e| BleError::Backend(e.to_string()))?;
        let adapter = match &self.adapter_name {
            Some(name) => session
                .adapter(name)
                .map_err(|e| BleError::Backend(e.to_string()))?,
            None => session
                .default_adapter()
                .await
                .map_err(|e| BleError::Backend(e.to_string()))?,
        };
        adapter
            .set_powered(true)
            .await
            .map_err(|e| BleError::Backend(e.to_string()))?;

        let app = self.build_application();
        let app_handle = adapter
            .serve_gatt_application(app)
            .await
            .map_err(|e| BleError::Backend(e.to_string()))?;

        let advertisement = Advertisement {
            service_uuids: [SERVICE_UUID].into_iter().collect(),
            local_name: Some(crate::protocol::ADVERTISED_NAME.to_string()),
            discoverable: Some(true),
            ..Default::default()
        };
        let adv_handle = adapter
            .advertise(advertisement)
            .await
            .map_err(|e| BleError::Backend(e.to_string()))?;

        // Register the pairing agent and open the pairing window before storing
        // the session, so a failure here unwinds the whole start (§15).
        if self.require_encryption() {
            self.arm_pairing(&session, &adapter).await?;
        }

        self._app = Some(app_handle);
        self._adv = Some(adv_handle);
        self._session = Some(session);
        Ok(())
    }

    async fn publish(&mut self, telemetry: Telemetry, flags: u8) -> Result<(), BleError> {
        if self._app.is_none() {
            return Err(BleError::Unavailable);
        }
        let message_type = telemetry.message_type();
        let payload = telemetry.encode_payload();
        let sender = self.channels.for_telemetry(&telemetry).clone();
        let cache = self.latest.for_telemetry(&telemetry).cloned();
        let (seq, timestamp) = self.dispatch(message_type, flags, &payload, &sender)?;

        // The Read+Notify telemetry characteristics (Status, Vitals, Fatigue)
        // cache one unfragmented framed value (header + payload) so a connecting
        // client's Read carries the same header the notify path does (PROTOCOL.md
        // §3, §6, §11), reusing the sequence and timestamp just sent.
        if let Some(cache) = cache {
            let framed = frame_unfragmented(message_type, seq, timestamp, flags, &payload);
            *cache.lock().await = framed;
        }
        Ok(())
    }

    async fn respond(&mut self, response: ControlResponse) -> Result<(), BleError> {
        if self._app.is_none() {
            return Err(BleError::Unavailable);
        }
        let payload = response.encode();
        let sender = self.channels.control.clone();
        self.dispatch(MessageType::ControlResponse, 0, &payload, &sender)?;
        Ok(())
    }

    fn control_requests(&mut self) -> Option<mpsc::Receiver<ControlRequest>> {
        self.control_rx.take()
    }
}

/// A read-only characteristic returning a fixed byte string (Protocol Info).
///
/// The read request carries the negotiated ATT MTU (BlueZ's `Fun`-based notify
/// callback does not), so this closure records it for the fragmenter. Clients
/// read Protocol Info on connect (`PROTOCOL.md` §14), so this is the first and
/// most reliable MTU sample.
fn read_characteristic(uuid: Uuid, value: Vec<u8>, mtu: Arc<AtomicUsize>) -> Characteristic {
    Characteristic {
        uuid,
        read: Some(CharacteristicRead {
            read: true,
            fun: Box::new(move |req| {
                mtu.store(req.mtu as usize, Ordering::Relaxed);
                let value = value.clone();
                async move { Ok(value) }.boxed()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A notify-only characteristic forwarding frames from `sender` (Pose, Point
/// Cloud). BlueZ has no encrypt-notify flag, so these carry no security flag of
/// their own; they are reachable only after an encrypted Control write enables
/// the stream (`PROTOCOL.md` §10, §15).
///
/// BlueZ's `Fun` notify callback exposes no MTU, so fragment sizing relies on the
/// MTU captured by the read/write characteristics (`read_characteristic`,
/// `readable_notify_characteristic`, `control_characteristic`); until the first
/// such request it defaults to [`DEFAULT_ATT_MTU`].
fn notify_characteristic(uuid: Uuid, sender: broadcast::Sender<Frame>) -> Characteristic {
    Characteristic {
        uuid,
        notify: Some(CharacteristicNotify {
            notify: true,
            method: CharacteristicNotifyMethod::Fun(Box::new(move |notifier| {
                let mut rx = sender.subscribe();
                async move { forward(notifier, &mut rx).await }.boxed()
            })),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A Read+Notify telemetry characteristic (Device Status, Vitals, Fatigue —
/// `PROTOCOL.md` §3). The Read returns the latest cached framed value (header +
/// payload) so a connecting central reads the current value the same way it
/// receives notifications (§6, §11); it is empty until the first publish. The
/// read closure also samples the negotiated MTU.
///
/// `encrypt_read` requires an encrypted (paired) link for the Read and, through
/// BlueZ's CCCD handling, for the notify subscription — the §15 gate that keeps
/// unpaired centrals from receiving telemetry.
fn readable_notify_characteristic(
    uuid: Uuid,
    sender: broadcast::Sender<Frame>,
    latest: Arc<Mutex<Vec<u8>>>,
    mtu: Arc<AtomicUsize>,
    encrypt_read: bool,
) -> Characteristic {
    Characteristic {
        uuid,
        read: Some(CharacteristicRead {
            read: true,
            encrypt_read,
            fun: Box::new(move |req| {
                mtu.store(req.mtu as usize, Ordering::Relaxed);
                let latest = latest.clone();
                async move { Ok(latest.lock().await.clone()) }.boxed()
            }),
            ..Default::default()
        }),
        notify: Some(CharacteristicNotify {
            notify: true,
            method: CharacteristicNotifyMethod::Fun(Box::new(move |notifier| {
                let mut rx = sender.subscribe();
                async move { forward(notifier, &mut rx).await }.boxed()
            })),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// The Stream Control characteristic: write-with-response requests parsed into
/// [`ControlRequest`]s and forwarded to the application; indicated responses
/// forwarded from `sender`.
fn control_characteristic(
    uuid: Uuid,
    sender: broadcast::Sender<Frame>,
    requests: mpsc::Sender<ControlRequest>,
    mtu: Arc<AtomicUsize>,
    encrypt_write: bool,
) -> Characteristic {
    Characteristic {
        uuid,
        write: Some(CharacteristicWrite {
            write: true,
            encrypt_write,
            method: CharacteristicWriteMethod::Fun(Box::new(move |value, req| {
                mtu.store(req.mtu as usize, Ordering::Relaxed);
                let requests = requests.clone();
                async move {
                    // Parse errors are dropped: a malformed control write should
                    // not tear down the ATT link. The client learns nothing came
                    // back (no Control Response) and can retry.
                    if let Ok(request) = ControlRequest::parse(&value) {
                        let _ = requests.send(request).await;
                    }
                    Ok(())
                }
                .boxed()
            })),
            ..Default::default()
        }),
        notify: Some(CharacteristicNotify {
            indicate: true,
            method: CharacteristicNotifyMethod::Fun(Box::new(move |notifier| {
                let mut rx = sender.subscribe();
                async move { forward(notifier, &mut rx).await }.boxed()
            })),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Forward frames from a broadcast receiver to a single subscriber's notifier
/// until it unsubscribes. Fragment sizing uses the MTU captured by the
/// read/write characteristics (BlueZ's `Fun` notify callback exposes none).
async fn forward(mut notifier: CharacteristicNotifier, rx: &mut broadcast::Receiver<Frame>) {
    loop {
        if notifier.is_stopped() {
            break;
        }
        match rx.recv().await {
            Ok(frame) => {
                if notifier.notify((*frame).clone()).await.is_err() {
                    break;
                }
            }
            // Lagged: we skipped some frames under backpressure. Keep going with
            // the newest — real-time frames are not retransmitted.
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
