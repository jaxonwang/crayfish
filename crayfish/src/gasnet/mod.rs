use crate::logging;
use crate::logging::*;
use crate::network::CollectiveOperator;
use crate::network::MessageHandler;
use crate::network::MessageSender;
use crate::network::Rank;
use crate::serialization;
use bit_vec::BitVec;
use futures::channel::oneshot;
use gex_sys::*;
use rustc_hash::FxHashMap;
use std::convert::TryInto;
use std::os::raw::*;
use std::ptr::null_mut;
use std::rc::Rc;
use std::slice;
use std::sync::mpsc;
use std::time;

extern crate gex_sys;
extern crate libc;
extern crate rustc_hash;

trait GexRank {
    fn gex_rank(&self) -> gex_Rank_t;
    fn from_gex_rank(rank: gex_Rank_t) -> Self;
}

impl GexRank for Rank {
    fn gex_rank(&self) -> gex_Rank_t {
        self.as_i32().try_into().unwrap()
    }
    fn from_gex_rank(rank: gex_Rank_t) -> Self {
        Self::new(rank.try_into().unwrap())
    }
}

type MessageId = usize;

enum NetworkOperation {
    Message(Rank, Vec<u8>),
    Barrier(oneshot::Sender<()>),
    Broadcast(Rank, *mut u8, usize, oneshot::Sender<()>),
    AllGather(Vec<u8>, oneshot::Sender<Vec<Vec<u8>>>),
}
unsafe impl Send for NetworkOperation {} // ptr is not send, but we are playing with unsafe!

enum CollectiveEventStatus {
    Done,
    Progress,
    Nothing,
}

trait CollectiveEventTrait {
    fn test(&mut self, ctx: &mut TransportContext) -> CollectiveEventStatus;
    fn notify(self: Box<Self>);
    fn set_id(&mut self, id: CollectiveEventId);
    /// should never assume the order of message received. So that recv can be called at anytime.
    fn recv(&mut self, data: &[u8]);
}

struct AllGatherEvent {
    id: CollectiveEventId,
    round: usize,              // at round i means round i-1 data received
    round_received: Vec<bool>, // round_recived[i + 1] == true when receive data from sender in round i
    buffer: Vec<Vec<u8>>,
    notifier: oneshot::Sender<Vec<Vec<u8>>>,
    ctx_data: Rc<ContextData>,
}

impl AllGatherEvent {
    fn new(
        input: Vec<u8>,
        notifier: oneshot::Sender<Vec<Vec<u8>>>,
        ctx_data: Rc<ContextData>,
    ) -> Self {
        let world_size = ctx_data.as_ref().world_size;
        let mut buffer = vec![vec![]; world_size];
        buffer[0] = input;
        let mut round_received = vec![false; Self::log2_ceiling(world_size) + 1];
        round_received[0] = true; // input is ready for the first round
        AllGatherEvent {
            id: 0,
            round: 0,
            round_received,
            buffer,
            notifier,
            ctx_data,
        }
    }

    fn log2_ceiling(num: usize) -> usize {
        debug_assert!(num > 0);
        std::mem::size_of::<usize>() * 8 - (num - 1).leading_zeros() as usize
    }
    fn here(&self) -> Rank {
        self.ctx_data.as_ref().local_rank
    }

    fn world_size(&self) -> usize {
        self.ctx_data.as_ref().world_size
    }

    fn term_round(&self) -> usize {
        Self::log2_ceiling(self.world_size())
    }
}

impl CollectiveEventTrait for AllGatherEvent {
    // bruck algorithm
    fn test(&mut self, ctx: &mut TransportContext) -> CollectiveEventStatus {
        let world_size = self.world_size();
        let term_round = self.term_round();

        // current round data sent but next round not received yet
        if !self.round_received[self.round] {
            return CollectiveEventStatus::Nothing;
        }

        while self.round_received[self.round] {
            if self.round == term_round {
                return CollectiveEventStatus::Done;
            }
            let this_round_chunk_num = 2_usize.pow(self.round as u32);
            // send only change propagated state, doesn't increase round
            let dst = world_size + self.here().as_usize() - 2usize.pow(self.round as u32);
            let dst = dst % world_size;
            // don't send data exceeding the total buffer length
            let chunk_num: usize = if this_round_chunk_num * 2 <= world_size {
                this_round_chunk_num
            } else {
                world_size - this_round_chunk_num
            };

            CollectiveContext::send(
                ctx,
                Rank::new(dst as i32),
                self.id,
                &(self.round, &self.buffer[..chunk_num]),
            );
            self.round += 1;
            // info!("round now increased to {}, received {:?}", self.round, self.round_received);
        }

        CollectiveEventStatus::Progress
    }
    fn notify(self: Box<Self>) {
        let mut this = self;
        // take the item with rank 0 to the begining
        let rank0_pos = this.world_size() - this.here().as_usize();
        let mut all: Vec<Vec<u8>> = this.buffer.drain(rank0_pos..).collect();
        // append the remaining
        all.append(&mut this.buffer);
        (*this).notifier.send(all).unwrap();
    }

    fn set_id(&mut self, id: CollectiveEventId) {
        self.id = id;
    }

    fn recv(&mut self, data: &[u8]) {
        // NOTE: bad design, handle stream here
        let (round, got): (usize, Vec<Vec<u8>>) = serialization::deserialize_from(data).unwrap();
        // error!("receive round {}", round);
        let start_pos = 2usize.pow(round as u32);
        for (i, item) in got.into_iter().enumerate() {
            self.buffer[start_pos + i] = item;
        }
        assert!(!self.round_received[round + 1]);
        self.round_received[round + 1] = true;
    }
}

struct GexEvent {
    id: CollectiveEventId,
    gex_handle: gex_Event_t,
    // make notifier a option since oneshot consume itself
    notifier: oneshot::Sender<()>,
}

impl GexEvent {
    fn new(gex_handle: gex_Event_t, notifier: oneshot::Sender<()>) -> Self {
        GexEvent {
            gex_handle,
            notifier,
            id: 0,
        }
    }
}

impl CollectiveEventTrait for GexEvent {
    fn test(&mut self, _: &mut TransportContext) -> CollectiveEventStatus {
        match gex_event_done(self.gex_handle) {
            true => CollectiveEventStatus::Done,
            false => CollectiveEventStatus::Nothing,
        }
    }
    fn notify(self: Box<Self>) {
        (*self).notifier.send(()).unwrap();
    }
    fn set_id(&mut self, id: CollectiveEventId) {
        self.id = id;
    }
    fn recv(&mut self, _data: &[u8]) {
        unreachable!()
    }
}

type MessageType = gex_AM_Arg_t;
const MESSAGE_TYPE_NORMAL: MessageType = 0;
const MESSAGE_TYPE_COLL: MessageType = 1;

struct TransportContext {
    team: gex_TM_t,
    segment_len: usize,
    endpoints_data: Vec<EndpointData>,

    max_global_long_request_len: usize,
    max_global_medium_request_len: usize,

    // for sending
    next_message_id: usize,

    // receive
    message_buffers: Vec<FxHashMap<MessageId, FragmentBuffer>>,
}

impl TransportContext {
    fn new(team: gex_TM_t) -> Self {
        TransportContext {
            team,
            segment_len: 0,
            endpoints_data: vec![],
            max_global_long_request_len: 0,
            max_global_medium_request_len: 0,
            next_message_id: 0,
            message_buffers: vec![],
        }
    }

    // interrupt safe, no malloc
    fn send(&mut self, dst: Rank, message_type: MessageType, message: &[u8]) {
        let message_type = message_type as gex_AM_Arg_t;
        trace!(
            "sending to {} with message type {}, len {}",
            dst,
            message_type,
            message.len()
        );
        // NOTE: not thread safe!
        let message_id = self.next_message_id;
        self.next_message_id += 1;
        let mut offset: usize = 0;
        let packet_size = self.max_global_medium_request_len;

        while offset < message.len() {
            let rest = &message[offset..];

            let src_addr = rest.as_ptr() as *const c_void;
            let send_size = usize::min(rest.len(), packet_size);

            let (a0, a1) = u64_to_i32_2(message.len() as u64);
            let (a2, a3) = u64_to_i32_2(offset as u64);
            let (a4, a5) = u64_to_i32_2(message_id as u64);
            #[rustfmt::skip]
                unsafe {
                    gex_am_reqeust_medium7(
                        self.team,
                        dst.gex_rank(),
                        MEDIUM_HANDLER_INDEX,
                        src_addr,
                        send_size as size_t,
                        gex_event_group(), //non-block
                        a0, a1, a2, a3, a4, a5, message_type
                    )
                };
            // trace!("send offset {} bytes {}", offset, send_size);
            offset += send_size;
        }
        gex_nbi_wait_am_lc();
    }
}

struct ContextData {
    local_rank: Rank,
    world_size: usize,
}

impl ContextData {
    fn new(local_rank: Rank, world_size: usize) -> Self {
        ContextData {
            local_rank,
            world_size,
        }
    }
}

pub struct CommunicationContext<T> {
    // endpoint: gex_EP_t, // thread safe handler
    // entry_table: Entrytable,
    cmd_args: Vec<String>,

    // receive
    message_handler: T,

    tctx: TransportContext,
    ctx_data: Rc<ContextData>,

    op_receiver: mpsc::Receiver<NetworkOperation>,
    op_sender: Option<mpsc::Sender<NetworkOperation>>, // set to option, will drop after run

    // for collective
    cctx: CollectiveContext,
}

type CollectiveEventId = usize;
#[derive(Default)]
struct CollectiveContext {
    collective_events: FxHashMap<CollectiveEventId, Box<dyn CollectiveEventTrait>>,
    associate_messages: FxHashMap<CollectiveEventId, Vec<Vec<u8>>>, // might recive message when event is not ready
    next_id: CollectiveEventId,
}

impl CollectiveContext {
    /// return true if there is progress
    fn poll(&mut self, ctx: &mut TransportContext) -> bool {
        let mut progress = false;
        let mut done = vec![];
        for (id, event) in self.collective_events.iter_mut() {
            match event.test(ctx) {
                CollectiveEventStatus::Done => {
                    done.push(*id);
                    progress = true;
                }
                CollectiveEventStatus::Progress => {
                    progress = true;
                }
                CollectiveEventStatus::Nothing => (),
            }
        }
        if done.is_empty() {
            return progress;
        }
        for id in done.iter() {
            trace!("collective event {} done.", id);
            let event = self.collective_events.remove(id).unwrap();
            event.notify();
        }
        progress
    }

    fn push(&mut self, event: impl CollectiveEventTrait + 'static) {
        self.next_id += 1;
        let mut event = event;
        event.set_id(self.next_id);
        self.collective_events.insert(self.next_id, Box::new(event));
        // has got some messages
        if let Some(messages) = self.associate_messages.remove(&self.next_id) {
            let event = self.collective_events.get_mut(&self.next_id).unwrap();
            for message in messages {
                event.recv(&message[..]);
            }
        }
    }

    fn recv(&mut self, _src: Rank, message: &[u8]) {
        let mut data = message;
        let event_id: CollectiveEventId = serialization::deserialize_from(&mut data).unwrap();
        // TODO: remove
        debug_assert!(data != message);
        if event_id > self.next_id {
            self.associate_messages
                .entry(event_id)
                .or_insert(vec![])
                .push(data.to_owned());
        } else {
            self.collective_events
                .get_mut(&event_id)
                .unwrap()
                .recv(data);
        }
    }

    fn send<T: ?Sized>(ctx: &mut TransportContext, dst: Rank, event_id: CollectiveEventId, t: &T)
    where
        T: serde::Serialize,
    {
        let mut data = vec![];
        serialization::serialize_into(&mut data, &event_id).unwrap();
        serialization::serialize_into(&mut data, t).unwrap();
        ctx.send(dst, MESSAGE_TYPE_COLL, &data);
    }
}
unsafe impl<T> Send for CommunicationContext<T> where T: MessageHandler {}

struct FragmentBuffer {
    buffer: Vec<u8>,
    fragment_size: usize,
    contigunous_end: usize,
    received_map: BitVec,
}

impl FragmentBuffer {
    fn with_length(length: usize, fragment_size: usize) -> Self {
        let bitmap_len = (length + fragment_size - 1) / fragment_size; // round up
        FragmentBuffer {
            buffer: vec![0; length],
            fragment_size,
            contigunous_end: 0,
            received_map: BitVec::from_elem(bitmap_len, false),
        }
    }

    fn save(&mut self, offset: usize, data: &[u8]) {
        if self.fragment_size == 0 {
            // only tail received
            self.fragment_size = data.len();
            let bitmap_len = (self.buffer.len() + self.fragment_size - 1) / self.fragment_size; // round upH
            self.received_map = BitVec::from_elem(bitmap_len, false);
            self.received_map.set(self.received_map.len() - 1, true); // set tail received
        }
        if cfg!(debug_assertions) {
            assert!(offset + data.len() <= self.buffer.len());
            assert_eq!(
                offset % self.fragment_size,
                0,
                "{} {}",
                offset,
                self.fragment_size
            );
            if offset + data.len() != self.buffer.len() {
                assert_eq!(data.len(), self.fragment_size);
            }
        }
        trace!(
            "buf got {:?}",
            if data.len() > 10 { &data[..10] } else { data }
        );
        self.buffer[offset..offset + data.len()].copy_from_slice(data);
        trace!(
            "write offset {}, data size {}, buf {:?}",
            offset,
            data.len(),
            &self.buffer[..10]
        );
        debug_assert_eq!(self.received_map[offset / self.fragment_size], false);
        self.received_map.set(offset / self.fragment_size, true);
        while self.contigunous_end < self.received_map.len()
            && self.received_map[self.contigunous_end]
        {
            self.contigunous_end += 1
        }
    }

    fn all_done(&self) -> bool {
        self.contigunous_end == self.received_map.len()
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct EndpointData {
    segment_addr: *mut c_void,
    segment_len: usize,
}

union B64 {
    u64_1: u64,
    i32_2: (i32, i32),
}

fn u64_to_i32_2(a: u64) -> (i32, i32) {
    unsafe { B64 { u64_1: a }.i32_2 }
}

fn i32_2_to_u64(a: i32, b: i32) -> u64 {
    unsafe { B64 { i32_2: (a, b) }.u64_1 }
}

// TODO: make this thread local and in refcell
const COM_CONTEXT_NULL: *mut () = null_mut::<()>();
static mut GLOBAL_CONTEXT_PTR: *mut () = COM_CONTEXT_NULL;

trait RankFromToken {
    fn from_token(token: gex_Token_t) -> Rank;
}

#[allow(clippy::too_many_arguments)]
fn _recv_message<TK: RankFromToken, T: MessageHandler>(
    token: gex_Token_t,
    buf: *const c_void,
    nbytes: size_t,
    arg0: gex_AM_Arg_t,
    arg1: gex_AM_Arg_t,
    arg2: gex_AM_Arg_t,
    arg3: gex_AM_Arg_t,
    arg4: gex_AM_Arg_t,
    arg5: gex_AM_Arg_t,
    arg6: gex_AM_Arg_t,
) {
    let src = TK::from_token(token);
    let src_idx = src.as_usize();

    let message_len = i32_2_to_u64(arg0, arg1) as usize;
    let offset = i32_2_to_u64(arg2, arg3) as usize;
    let message_id = i32_2_to_u64(arg4, arg5) as usize;
    let context = unsafe { &mut *(GLOBAL_CONTEXT_PTR as *mut CommunicationContext<T>) };

    trace!(
        "medium recv {} bytes from {} at mem {:p}",
        nbytes,
        src,
        buf,
    );

    let call_handler = |buf: &[u8]| unsafe {
        debug_assert!(
            GLOBAL_CONTEXT_PTR != COM_CONTEXT_NULL,
            "Context pointer is null"
        );
        let message_type = arg6 as MessageType;
        (*(GLOBAL_CONTEXT_PTR as *mut CommunicationContext<T>)).recv(src, message_type, buf);
    };

    let packet_size = context.tctx.max_global_medium_request_len;
    if message_len <= packet_size{
        let buf = unsafe { slice::from_raw_parts(buf as *const u8, message_len) };
        // whole message received
        trace!("receive message of len {} from {}", message_len, src);
        debug_assert!(offset == 0);
        debug_assert!(message_len == nbytes as usize);
        call_handler(buf);
        return;
    }

    // now message is fragmented

    let buf = unsafe { slice::from_raw_parts(buf as *const u8, nbytes as usize) };
    let fg_buffer = context.tctx.message_buffers[src_idx]
        .entry(message_id)
        .or_insert_with(|| FragmentBuffer::with_length(message_len, packet_size));
    fg_buffer.save(offset, buf);

    if fg_buffer.all_done() {
        let buf = &fg_buffer.buffer[..];
        trace!("receive message of len {} from {}", message_len, src);
        call_handler(buf);
        context.tctx.message_buffers[src_idx].remove(&message_id);
    }
}

extern "C" fn recv_message<T: MessageHandler>(
    token: gex_Token_t,
    buf: *const c_void,
    nbytes: size_t,
    a0: gex_AM_Arg_t,
    a1: gex_AM_Arg_t,
    a2: gex_AM_Arg_t,
    a3: gex_AM_Arg_t,
    a4: gex_AM_Arg_t,
    a5: gex_AM_Arg_t,
    a6: gex_AM_Arg_t,
) {
    struct Getter;
    impl RankFromToken for Getter {
        fn from_token(token: gex_Token_t) -> Rank {
            let t_info = gex_token_info(token);
            Rank::from_gex_rank(t_info.gex_srcrank)
        }
    }
    _recv_message::<Getter, T>(token, buf, nbytes, a0, a1, a2, a3, a4, a5, a6);
}

const MEDIUM_HANDLER_INDEX: gex_AM_Index_t = GEX_AM_INDEX_BASE as u8;

fn prepare_entry_table<T: MessageHandler>() -> Entrytable {
    let mut tb = Entrytable::new();
    unsafe {
        tb.add_medium_req(
            MEDIUM_HANDLER_INDEX,
            recv_message::<T> as *const (),
            7,
            Some("recv_message"),
        );
    }
    tb
}

impl<T> CommunicationContext<T>
where
    T: MessageHandler,
{
    pub fn new(handler: T) -> Self {
        assert!(
            unsafe { GLOBAL_CONTEXT_PTR == COM_CONTEXT_NULL },
            "Should only one instance!"
        );

        let (args, _, ep, tm) = gex_client_init();

        let mut tb = prepare_entry_table::<T>();

        // register the table
        gex_register_entries(ep, &mut tb);

        let (tx, rx) = mpsc::channel();
        let context = CommunicationContext {
            cmd_args: args,
            message_handler: handler,
            tctx: TransportContext::new(tm),
            ctx_data: Rc::new(ContextData::new(
                Rank::from_gex_rank(gax_system_query_jobrank()),
                gax_system_query_jobsize() as usize,
            )),
            op_receiver: rx,
            op_sender: Some(tx),
            cctx: Default::default(),
        };

        logging::set_global_id(context.here().as_i32());
        context
    }

    pub fn init(&mut self) {
        // the segment is devided into world_size chunks, reserved for each peer
        // init the segment
        let max_seg_len = gasnet_get_max_local_segment_size();
        // limit each rank use 64MB at most
        let seg_len = usize::min(max_seg_len, 64 * 1024 * 1024);
        let seg = gex_segment_attach(self.tctx.team, seg_len);
        self.tctx.segment_len = gax_segment_query_size(seg);
        debug!(
            "Try to set buffer length: {} KB. Got {} KB",
            max_seg_len / 1024,
            self.tctx.segment_len / 1024,
        );

        // set max global long
        self.tctx.max_global_long_request_len = gex_am_max_global_request_long(self.tctx.team);
        // TODO: current assume symmetric machines
        self.tctx.max_global_medium_request_len = gex_am_max_global_request_medium(self.tctx.team);
        debug!(
            "Setting max global long reqeust length to {}",
            self.tctx.max_global_long_request_len
        );
        debug!(
            "Setting max global medium reqeust length to {}",
            self.tctx.max_global_medium_request_len
        );
        // TODO: warn if request len too short for udp, ibv...

        // set endpoint addr offset
        for i in 0..self.world_size() {
            let (segment_addr, segment_len) =
                gex_ep_query_bound_segment(self.tctx.team, i as gex_Rank_t);
            // assert_eq!(segment_len, self.segment_len); TODO: different machine.
            self.tctx.endpoints_data.push(EndpointData {
                segment_addr,
                segment_len,
            });
            // prepare buffer
            self.tctx.message_buffers.push(FxHashMap::default());
        }
        debug!("Endpoint data: {:?}", self.tctx.endpoints_data);
        // set proper ptr
        unsafe {
            // WARN: danger!
            GLOBAL_CONTEXT_PTR = self as *mut CommunicationContext<T> as *mut ();
        }

        // in case some finish init fast and send message before the pointer is set
        let event = gex_coll_barrier_nb(self.tctx.team);
        gex_event_wait(event);
    }

    fn barrier(&mut self, notify: oneshot::Sender<()>) {
        let event = gex_coll_barrier_nb(self.tctx.team);
        self.push_event(GexEvent::new(event, notify));
    }

    fn push_event(&mut self, event: impl CollectiveEventTrait + 'static) {
        self.cctx.push(event)
    }

    fn broadcast(&mut self, root: Rank, data: *mut u8, len: usize, notify: oneshot::Sender<()>) {
        let event = unsafe {
            gex_coll_broadcast_nb(
                self.tctx.team,
                root.gex_rank(),
                data as *mut c_void,
                data as *mut c_void,
                len as u64,
            )
        };
        self.push_event(GexEvent::new(event, notify));
    }

    fn all_gather(&mut self, data: Vec<u8>, notify: oneshot::Sender<Vec<Vec<u8>>>) {
        self.push_event(AllGatherEvent::new(data, notify, self.ctx_data.clone()));
    }

    /// return true if there is progress
    fn poll_collective_events(&mut self) -> bool {
        self.cctx.poll(&mut self.tctx)
    }

    pub fn run(&mut self) {
        use mpsc::TryRecvError::*;
        // drop sender, otherwise the channel will never be closed
        self.op_sender = None;

        // message loop
        let mut sleep_us = time::Duration::from_micros(1);
        let mut progress;
        loop {
            gasnet_ampoll();
            progress = self.poll_collective_events();
            match self.op_receiver.try_recv() {
                Ok(op) => {
                    match op {
                        NetworkOperation::Message(dst, msg) => {
                            self.tctx.send(dst, MESSAGE_TYPE_NORMAL, &msg[..])
                        }
                        NetworkOperation::Barrier(notify) => self.barrier(notify),
                        NetworkOperation::Broadcast(root, data, len, notify) => {
                            self.broadcast(root, data, len, notify)
                        }
                        NetworkOperation::AllGather(data, notify) => self.all_gather(data, notify),
                    };
                    progress = true;
                }
                Err(Empty) => {}
                Err(Disconnected) => break, // the upper layer stop fist
            }

            if !progress {
                std::thread::sleep(sleep_us);
                // max sleep time should not be too large otherwise timeout
                if sleep_us < time::Duration::from_millis(1) {
                    sleep_us *= 2;
                }
            } else {
                sleep_us = time::Duration::from_micros(1);
            }
        }

        info!("Shuting down network.");
    }

    // fn wait_all_send_handled(&self) {
    //     for idx in 0..self.world_size {
    //         if idx != self.local_rank.as_usize() {
    //             self.wait_send_handled(idx);
    //         }
    //     }
    // }
    /// this is called in callback function
    pub fn recv(&mut self, src: Rank, message_type: MessageType, message: &[u8]) {
        match message_type {
            MESSAGE_TYPE_NORMAL => (self.message_handler)(src, message),
            MESSAGE_TYPE_COLL => self.cctx.recv(src, message),
            _ => unreachable!(),
        }
    }

    pub fn single_sender(&self) -> SingleSender {
        // TODO not litmit to single now
        SingleSender {
            message_chan: self.op_sender.as_ref().unwrap().clone(),
        }
    }

    pub(crate) fn collective_operator(&self) -> GexCollectiveOperator {
        GexCollectiveOperator {
            sender: self.op_sender.as_ref().unwrap().clone(),
            ongoing_barrier: false,
        }
    }

    pub fn cmd_args(&self) -> &[String] {
        &self.cmd_args[..]
    }

    pub fn here(&self) -> Rank {
        self.ctx_data.as_ref().local_rank
    }

    pub fn world_size(&self) -> usize {
        self.ctx_data.as_ref().world_size
    }
}

pub struct SingleSender {
    message_chan: mpsc::Sender<NetworkOperation>,
}

impl MessageSender for SingleSender {
    fn send_msg(&self, dst: Rank, message: Vec<u8>) {
        self.message_chan
            .send(NetworkOperation::Message(dst, message))
            .unwrap();
    }
}

pub(crate) struct GexCollectiveOperator {
    sender: mpsc::Sender<NetworkOperation>,
    ongoing_barrier: bool,
}

const BEEN_WAITED_ERR_MSG: &str = "Barrier has been waited!";
impl CollectiveOperator for GexCollectiveOperator {
    fn barrier(&mut self) -> oneshot::Receiver<()> {
        assert!(!self.ongoing_barrier, "{}", BEEN_WAITED_ERR_MSG);
        let (tx, rx) = oneshot::channel();
        self.sender.send(NetworkOperation::Barrier(tx)).unwrap();
        self.ongoing_barrier = true;
        rx
    }

    fn barrier_done(&mut self) {
        self.ongoing_barrier = false;
    }

    fn broadcast(&self, root: Rank, bytes: *mut u8, size: usize) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(NetworkOperation::Broadcast(root, bytes, size, tx))
            .unwrap();
        rx
    }

    fn all_gather(&self, bytes: Vec<u8>) -> oneshot::Receiver<Vec<Vec<u8>>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(NetworkOperation::AllGather(bytes, tx))
            .unwrap();
        rx
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use once_cell::sync::Lazy;
    use rand::prelude::*;
    use std::ptr::null;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::MutexGuard;

    const MAX_REQUEST_SIZE: usize = 100;

    fn fake_context<T: MessageHandler>(f: T) -> CommunicationContext<T> {
        let (tx, rx) = mpsc::channel();
        let tctx = TransportContext {
            team: null::<*const ()>() as gex_TM_t,
            segment_len: 0,
            endpoints_data: vec![EndpointData {
                segment_addr: null_mut::<c_void>(),
                segment_len: 512,
            }],
            max_global_long_request_len: MAX_REQUEST_SIZE,
            max_global_medium_request_len: MAX_REQUEST_SIZE,
            next_message_id: 0,
            message_buffers: vec![FxHashMap::default()],
        };
        CommunicationContext {
            cmd_args: vec![],
            message_handler: f,
            tctx,
            ctx_data: Rc::new(ContextData::new(Rank::new(0), 0)),
            op_receiver: rx,
            op_sender: Some(tx),
            cctx: Default::default(),
        }
    }
    fn set_ptr<T>(ctx: &mut CommunicationContext<T>) {
        unsafe {
            GLOBAL_CONTEXT_PTR = ctx as *mut CommunicationContext<T> as *mut ();
        }
    }

    type MessageCalls = Vec<(*const c_void, size_t, i32, i32, i32, i32, i32, i32)>;
    fn generate_recv_calls(message_id: usize, message_payload: &[u8]) -> MessageCalls {
        let mut calls = vec![];
        let mut offset: usize = 0;
        let message_len = message_payload.len();
        while offset < message_len {
            let (a, b) = u64_to_i32_2(message_len as u64);
            let (c, d) = u64_to_i32_2(offset as u64);
            let (e, f) = u64_to_i32_2(message_id as u64);
            let buf = message_payload[offset..].as_ptr() as *const c_void;
            let nbytes = usize::min(message_len - offset, MAX_REQUEST_SIZE) as size_t;
            calls.push((buf, nbytes, a, b, c, d, e, f));
            offset += nbytes as usize;
        }
        assert_eq!(offset, message_len);
        calls
    }

    // fake src getter
    struct FakeGetter;

    impl RankFromToken for FakeGetter {
        fn from_token(_: gex_Token_t) -> Rank {
            Rank::new(0)
        }
    }

    static CONTEXT_TEST_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

    struct ContextSetUp<'a, T> {
        _guard: MutexGuard<'a, bool>,
        ctx: CommunicationContext<T>,
    }
    impl<'a, T> ContextSetUp<'a, T>
    where
        T: MessageHandler,
    {
        fn new(callback: T) -> Self {
            let _guard = CONTEXT_TEST_LOCK.lock().unwrap();
            let ret = ContextSetUp {
                _guard,
                ctx: fake_context(callback),
            };
            ret
        }
    }

    fn _test_single_long_receiver<T: MessageHandler>(callback: T, called: Arc<AtomicBool>) {
        let mut c = ContextSetUp::new(callback);
        set_ptr(&mut c.ctx);

        let message_payload: Vec<u8> = vec![1; 1234];

        let mut calls = generate_recv_calls(1, &message_payload[..]);

        let test_message_order = |calls: &MessageCalls| {
            called.store(false, Ordering::Relaxed);
            for (buf, nbytes, a, b, c, d, e, f) in calls.clone().into_iter().take(calls.len() - 1) {
                #[rustfmt::skip]
                _recv_message::<FakeGetter, T>(
                    null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d, e, f, 0);
                assert_eq! {called.load(Ordering::Relaxed), false};
            }
            let (buf, nbytes, a, b, c, d, e, f) = calls[calls.len() - 1].clone();
            #[rustfmt::skip]
            _recv_message::<FakeGetter, T>(
                null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d, e, f, 0);
            assert_eq! {called.load(Ordering::Relaxed), true};
        };
        test_message_order(&calls);
        let reversed = calls.iter().rev().cloned().collect();
        test_message_order(&reversed);
        let mut rng = rand::thread_rng();
        for _ in 0..400 {
            calls.shuffle(&mut rng);
            test_message_order(&calls);
        }
    }

    #[test]
    fn test_single_long_receiver() {
        let called = Arc::new(AtomicBool::new(false));
        let called1 = called.clone();
        let callback = |_a: Rank, _b: &[u8]| {
            called1.store(true, Ordering::Relaxed);
        };
        _test_single_long_receiver(callback, called);
    }

    fn _test_many_long_receiver<T: MessageHandler>(callback: T, message_payload: Vec<u8>) {
        let mut rng = thread_rng();
        let mut c = ContextSetUp::new(callback);
        set_ptr(&mut c.ctx);

        let mut calls = vec![];
        for i in 0..100 {
            calls.extend_from_slice(&generate_recv_calls(i, &message_payload[..])[..]);
        }
        calls.shuffle(&mut rng);
        for (buf, nbytes, a, b, c, d, e, f) in calls.clone() {
            #[rustfmt::skip]
            _recv_message::<FakeGetter, T>(
                null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d, e, f, 0);
        }
    }

    #[test]
    fn test_many_long_receiver() {
        let mut rng = thread_rng();
        let message_payload: Vec<u8> = (0..1234).map(|_| rng.gen()).collect();
        let message_payload1 = message_payload.clone();
        let callback = move |_a: Rank, data: &[u8]| {
            assert_eq!(data, &message_payload1[..]);
        };
        _test_many_long_receiver(callback, message_payload);
    }

    fn _test_short_long_receiver<T: MessageHandler>(callback: T, message_payload: Vec<u8>) {
        let mut c = ContextSetUp::new(callback);
        set_ptr(&mut c.ctx);

        let calls = generate_recv_calls(3, &message_payload[..]);
        assert_eq!(calls.len(), 1);
        let (buf, nbytes, a, b, c, d, e, f) = calls[0];
        #[rustfmt::skip]
        _recv_message::<FakeGetter, T>(
            null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d, e, f, 0);
    }

    #[test]
    fn test_short_long_receiver() {
        let mut rng = thread_rng();
        let message_payload: Vec<u8> = (0..50).map(|_| rng.gen()).collect();
        let message_payload1 = message_payload.clone();
        let callback = move |_a: Rank, data: &[u8]| {
            assert_eq!(data, &message_payload1[..]);
        };
        _test_short_long_receiver(callback, message_payload);
    }
}
