use crate::logging;
use crate::logging::*;
use crate::network::CollectiveOperator;
use crate::network::MessageHandler;
use crate::network::MessageSender;
use crate::network::Rank;
use bit_vec::BitVec;
use futures::channel::oneshot;
use gex_sys::*;
use rustc_hash::FxHashMap;
use std::convert::TryInto;
use std::os::raw::*;
use std::ptr::null_mut;
use std::slice;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
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
}
unsafe impl Send for NetworkOperation {} // ptr is not send, but we are playing with unsafe!

pub struct CommunicationContext<T> {
    // endpoint: gex_EP_t, // thread safe handler
    team: gex_TM_t,
    // entry_table: Entrytable,
    cmd_args: Vec<String>,

    message_handler: T,

    segment_len: usize,
    endpoints_data: Vec<EndpointData>,

    max_global_long_request_len: usize,
    max_global_medium_request_len: usize,

    local_rank: Rank,
    world_size: usize,

    message_buffers: Vec<FxHashMap<MessageId, FragmentBuffer>>,

    // for sending
    next_message_id: usize,
    op_receiver: mpsc::Receiver<NetworkOperation>,
    op_sender: Option<mpsc::Sender<NetworkOperation>>, // set to option, will drop after run
    sent_fragment_id: Vec<usize>,
    ack_fragment_id: Vec<AtomicUsize>,

    // for collective
    collective_events: Vec<(gex_Event_t, oneshot::Sender<()>)>,
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
        error!(
            "buf got {:?}",
            if data.len() > 10 { &data[..10] } else { data }
        );
        self.buffer[offset..offset + data.len()].copy_from_slice(data);
        error!(
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

extern "C" fn recv_medium<T: MessageHandler>(
    token: gex_Token_t,
    buf: *const c_void,
    nbytes: size_t,
) {
    let t_info = gex_token_info(token);
    let src = Rank::from_gex_rank(t_info.gex_srcrank);
    // trace!("medium recv {} bytes from {} at mem {:?}", nbytes, src, buf);

    unsafe {
        let buf = slice::from_raw_parts(buf as *const u8, nbytes.try_into().unwrap());
        debug_assert!(
            GLOBAL_CONTEXT_PTR != COM_CONTEXT_NULL,
            "Context pointer is null"
        );
        ((*(GLOBAL_CONTEXT_PTR as *mut CommunicationContext<T>)).message_handler)(src, buf);
    }
}

trait RankFromToken {
    fn from_token(token: gex_Token_t) -> Rank;
    fn reply_short(token: gex_Token_t);
}

#[allow(clippy::too_many_arguments)]
fn _recv_long<TK: RankFromToken, T: MessageHandler>(
    token: gex_Token_t,
    buf: *const c_void,
    _nbytes: size_t,
    arg0: gex_AM_Arg_t,
    arg1: gex_AM_Arg_t,
    arg2: gex_AM_Arg_t,
    arg3: gex_AM_Arg_t,
    arg4: gex_AM_Arg_t,
    arg5: gex_AM_Arg_t,
) {
    let src = TK::from_token(token);
    let src_idx = src.as_usize();

    let message_len = i32_2_to_u64(arg0, arg1) as usize;
    let offset = i32_2_to_u64(arg2, arg3) as usize;
    let message_id = i32_2_to_u64(arg4, arg5) as usize;
    let context = unsafe { &mut *(GLOBAL_CONTEXT_PTR as *mut CommunicationContext<T>) };
    let chunk_size = context.endpoints_data[context.local_rank.as_usize()].segment_len;

    trace!(
        "long recv {} bytes from {} at mem {:p} {}",
        _nbytes,
        src,
        buf,
        chunk_size
    );

    let call_handler = |buf: &[u8]| unsafe {
        debug_assert!(
            GLOBAL_CONTEXT_PTR != COM_CONTEXT_NULL,
            "Context pointer is null"
        );
        ((*(GLOBAL_CONTEXT_PTR as *mut CommunicationContext<T>)).message_handler)(src, buf);
    };

    if message_len <= chunk_size {
        let buf = unsafe { slice::from_raw_parts(buf as *const u8, message_len) };
        // whole message received
        debug!("receive message of len {} from {}", message_len, src);
        debug_assert!(offset == 0);
        call_handler(buf);
        TK::reply_short(token);
        return;
    }

    // now message is fragmented

    let buf = unsafe { slice::from_raw_parts(buf as *const u8, chunk_size) };
    let buf = &buf[..usize::min(message_len - offset, chunk_size)];
    let fg_buffer = context.message_buffers[src_idx]
        .entry(message_id)
        .or_insert_with(|| FragmentBuffer::with_length(message_len, chunk_size));
    fg_buffer.save(offset, buf);

    if fg_buffer.all_done() {
        let buf = &fg_buffer.buffer[..];
        call_handler(buf);
        context.message_buffers[src_idx].remove(&message_id);
    }
    TK::reply_short(token);
}

extern "C" fn recv_long<T: MessageHandler>(
    token: gex_Token_t,
    buf: *const c_void,
    nbytes: size_t,
    a0: gex_AM_Arg_t,
    a1: gex_AM_Arg_t,
    a2: gex_AM_Arg_t,
    a3: gex_AM_Arg_t,
    a4: gex_AM_Arg_t,
    a5: gex_AM_Arg_t,
) {
    struct Getter;
    impl RankFromToken for Getter {
        fn from_token(token: gex_Token_t) -> Rank {
            let t_info = gex_token_info(token);
            Rank::from_gex_rank(t_info.gex_srcrank)
        }
        fn reply_short(token: gex_Token_t) {
            gex_am_reply_short0(token, ACK_REPLY_INDEX);
        }
    }
    _recv_long::<Getter, T>(token, buf, nbytes, a0, a1, a2, a3, a4, a5);
}

/// when received reply from remote indicating the whole message is received
extern "C" fn ack_reply<T: MessageHandler>(token: gex_Token_t) {
    let t_info = gex_token_info(token);
    let src = t_info.gex_srcrank as usize;

    let context = unsafe { &mut *(GLOBAL_CONTEXT_PTR as *mut CommunicationContext<T>) };

    let _a = context.ack_fragment_id[src].fetch_add(1, Ordering::Release);
}

const MEDIUM_HANDLER_INDEX: gex_AM_Index_t = GEX_AM_INDEX_BASE as u8;
const LONG_HANDLER_INDEX: gex_AM_Index_t = GEX_AM_INDEX_BASE as u8 + 1;
const ACK_REPLY_INDEX: gex_AM_Index_t = GEX_AM_INDEX_BASE as u8 + 2;
fn prepare_entry_table<T: MessageHandler>() -> Entrytable {
    let mut tb = Entrytable::new();
    unsafe {
        tb.add_medium_req(
            MEDIUM_HANDLER_INDEX,
            recv_medium::<T> as *const (),
            0,
            Some("recv_medium"),
        );
        tb.add_long_req(
            LONG_HANDLER_INDEX,
            recv_long::<T> as *const (),
            6,
            Some("recv_long"),
        );
        tb.add_short_reply(
            ACK_REPLY_INDEX,
            ack_reply::<T> as *const (),
            0,
            Some("ack_reply"),
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
            team: tm,
            endpoints_data: vec![],
            cmd_args: args,
            message_handler: handler,
            segment_len: 0,
            max_global_long_request_len: 0,
            max_global_medium_request_len: 0,
            local_rank: Rank::from_gex_rank(gax_system_query_jobrank()),
            world_size: gax_system_query_jobsize() as usize,
            message_buffers: vec![],
            op_receiver: rx,
            op_sender: Some(tx),
            next_message_id: 0,
            sent_fragment_id: vec![],
            ack_fragment_id: vec![],
            collective_events: vec![],
        };

        logging::set_global_id(context.here().as_i32());
        context
    }

    pub fn init(&mut self) {
        // the segment is devided into world_size chunks, reserved for each peer
        // init the segment
        // let seg_len: usize = 2 * 1024 * 1024 * 1024; // 2 GB
        let max_seg_len = gasnet_get_max_local_segment_size();
        // let seg_len = usize::min(seg_len, max_seg_len);
        let seg_len = max_seg_len;
        let seg_len = seg_len / GASNET_PAGESIZE as usize / self.world_size
            * GASNET_PAGESIZE as usize
            * self.world_size; // round to each chunk
        let chunk_size = seg_len / self.world_size;
        assert!(
            chunk_size > GASNET_PAGESIZE as usize,
            "Segment length: {} is too small for {} peers",
            seg_len,
            self.world_size
        );
        let seg = gex_segment_attach(self.team, seg_len);
        self.segment_len = gax_segment_query_size(seg);
        debug!(
            "Max buffer length: {} KB. Round buffer length to {} KB, chunk size:{} KB",
            max_seg_len / 1024,
            self.segment_len / 1024,
            chunk_size / 1024
        );

        // set max global long
        self.max_global_long_request_len = gex_am_max_global_request_long(self.team);
        // TODO: current assume symmetric machines
        self.max_global_medium_request_len = gex_am_max_global_request_medium(self.team);
        debug!(
            "Setting max global long reqeust length to {}",
            self.max_global_long_request_len
        );
        debug!(
            "Setting max global medium reqeust length to {}",
            self.max_global_medium_request_len
        );
        // TODO: warn if request len too short for udp, ibv...

        // set endpoint addr offset
        for i in 0..self.world_size {
            let (segment_addr, _segment_len) =
                gex_ep_query_bound_segment(self.team, i as gex_Rank_t);
            // assert_eq!(segment_len, self.segment_len); TODO: different machine.
            self.endpoints_data.push(EndpointData {
                segment_addr: unsafe {
                    segment_addr.offset(
                        (self.local_rank.as_usize() * chunk_size)
                            .try_into()
                            .unwrap(),
                    )
                },
                segment_len: chunk_size,
            });
            // prepare buffer
            self.message_buffers.push(FxHashMap::default());
            // prepare frangment ids
            self.sent_fragment_id.push(0);
            self.ack_fragment_id.push(AtomicUsize::new(0));
        }
        debug!("Endpoint data: {:?}", self.endpoints_data);
        // set proper ptr
        unsafe {
            // WARN: danger!
            GLOBAL_CONTEXT_PTR = self as *mut CommunicationContext<T> as *mut ();
        }

        // in case some finish init fast and send message before the pointer is set
        let event = gex_coll_barrier_nb(self.team);
        gex_event_wait(event);
    }

    fn barrier(&mut self, notify: oneshot::Sender<()>) {
        let event = gex_coll_barrier_nb(self.team);
        self.push_event(event, notify);
    }

    fn push_event(&mut self, event: gex_Event_t, notify: oneshot::Sender<()>) {
        self.collective_events.push((event, notify))
    }

    fn broadcast(&mut self, root: Rank, data: *mut u8, len: usize, notify: oneshot::Sender<()>) {
        let event = unsafe {
            gex_coll_broadcast_nb(
                self.team,
                root.gex_rank(),
                data as *mut c_void,
                data as *mut c_void,
                len as u64,
            )
        };
        self.push_event(event, notify)
    }

    fn poll_collective_events(&mut self) {
        let mut done = vec![];
        for (i, (event, _notify)) in self.collective_events.iter().enumerate() {
            if gex_event_done(*event) {
                // if success
                done.push(i)
            }
        }
        if done.is_empty() {
            return;
        }
        let mut last = self.collective_events.len();
        for i in done.iter().rev() {
            last -= 1;
            self.collective_events.swap(*i, last);
        }
        let remain = self.collective_events.len() - done.len();
        self.collective_events
            .drain(remain..self.collective_events.len())
            .for_each(|(_, n)| n.send(()).unwrap());
    }

    pub fn run(&mut self) {
        use mpsc::TryRecvError::*;
        // drop sender, otherwise the channel will never be closed
        self.op_sender = None;

        // message loop
        let mut sleep_us = time::Duration::from_micros(1);
        loop {
            let mut nothing = false;
            gasnet_ampoll();
            self.poll_collective_events();
            match self.op_receiver.try_recv() {
                Ok(NetworkOperation::Message(dst, msg)) => self.send(dst, &msg[..]),
                Ok(NetworkOperation::Barrier(notify)) => self.barrier(notify),
                Ok(NetworkOperation::Broadcast(root, data, len, notify)) => {
                    self.broadcast(root, data, len, notify)
                }
                Err(Empty) => {
                    nothing = true;
                }
                Err(Disconnected) => break, // the upper layer stop fist
            }

            if nothing {
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

    fn wait_send_handled(&self, dst_idx: usize) {
        // loop until ack
        // TODO: sleep
        while self.ack_fragment_id[dst_idx].load(Ordering::Acquire)
            != self.sent_fragment_id[dst_idx]
        {
            gasnet_ampoll();
        }
    }

    // fn wait_all_send_handled(&self) {
    //     for idx in 0..self.world_size {
    //         if idx != self.local_rank.as_usize() {
    //             self.wait_send_handled(idx);
    //         }
    //     }
    // }

    // interrupt safe, no malloc
    pub fn send(&mut self, dst: Rank, message: &[u8]) {
        debug!("sending to {} with message len {}", dst, message.len());
        // NOTE: not thread safe!
        if message.len() < self.max_global_medium_request_len {
            unsafe {
                gex_am_reqeust_medium0(
                    self.team,
                    dst.gex_rank(),
                    MEDIUM_HANDLER_INDEX,
                    message.as_ptr() as *const c_void,
                    message.len() as size_t,
                    gex_event_now(), // block
                )
            };
        } else {
            let message_id = self.next_message_id;
            self.next_message_id += 1;
            let mut offset: usize = 0;
            let dst_idx = dst.as_usize();
            let chunk_size = self.endpoints_data[dst_idx].segment_len;
            let packet_size = self.max_global_long_request_len;

            while offset < message.len() {
                // wait till last finished
                self.wait_send_handled(dst_idx);
                let rest = &message[offset..];
                let old_offset = offset;

                if rest.len() > packet_size {
                    let src_addr = message[packet_size..].as_ptr() as *const c_void;
                    let dst_addr =
                        unsafe { self.endpoints_data[dst_idx].segment_addr.add(packet_size) };
                    let send_size = usize::min(rest.len() - packet_size, chunk_size);
                    unsafe {
                        gex_rma_putblocking(
                            self.team,
                            dst.gex_rank(),
                            dst_addr,
                            src_addr,
                            send_size as u64,
                        );
                    }
                    offset += send_size;
                }

                let (a0, a1) = u64_to_i32_2(message.len() as u64);
                let (a2, a3) = u64_to_i32_2(old_offset as u64);
                let (a4, a5) = u64_to_i32_2(message_id as u64);
                let send_slice = &message[old_offset..];
                let send_size = usize::min(send_slice.len(), packet_size);
                #[rustfmt::skip]
                unsafe {
                    gex_am_reqeust_long6(
                        self.team,
                        dst.gex_rank(),
                        LONG_HANDLER_INDEX,
                        send_slice.as_ptr() as *const c_void,
                        send_size as size_t,
                        self.endpoints_data[dst_idx].segment_addr,
                        0,
                        gex_event_group(), // non block
                        a0, a1, a2, a3, a4, a5
                    )
                };
                // trace!("send offset {} bytes {}", offset, send_size);
                offset += send_size;
                self.sent_fragment_id[dst_idx] += 1;
                gex_nbi_wait_am_lc(); // wait until message buffer can be safely released
            }
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
        self.local_rank
    }

    pub fn world_size(&self) -> usize {
        self.world_size
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

    const MAX_CHUNK_SIZE: usize = 100;

    fn fake_context<T: MessageHandler>(f: T) -> CommunicationContext<T> {
        let (tx, rx) = mpsc::channel();
        CommunicationContext {
            team: null::<*const ()>() as gex_TM_t,
            endpoints_data: vec![EndpointData {
                segment_addr: null_mut::<c_void>(),
                segment_len: MAX_CHUNK_SIZE,
            }],
            cmd_args: vec![],
            message_handler: f,
            segment_len: 0,
            max_global_long_request_len: 1024,
            max_global_medium_request_len: 512,
            local_rank: Rank::new(0),
            world_size: 0,
            message_buffers: vec![FxHashMap::default()],
            op_receiver: rx,
            op_sender: Some(tx),
            next_message_id: 0,
            sent_fragment_id: vec![0],
            ack_fragment_id: vec![AtomicUsize::new(0)],
            collective_events: vec![],
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
            let nbytes = usize::min(message_len - offset, MAX_CHUNK_SIZE) as size_t;
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
        fn reply_short(_: gex_Token_t) {}
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
                _recv_long::<FakeGetter, T>(
                    null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d, e, f,);
                assert_eq! {called.load(Ordering::Relaxed), false};
            }
            let (buf, nbytes, a, b, c, d, e, f) = calls[calls.len() - 1].clone();
            #[rustfmt::skip]
            _recv_long::<FakeGetter, T>(
                null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d, e, f,);
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
            _recv_long::<FakeGetter, T>(
                null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d, e, f,);
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
        _recv_long::<FakeGetter, T>(
            null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d, e, f,);
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
