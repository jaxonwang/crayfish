use crate::logging;
use crate::logging::*;
use gex_sys::*;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::convert::TryInto;
use std::fmt;
use std::os::raw::*;
use std::ptr::null_mut;
use std::slice;
use std::sync::mpsc;

extern crate gex_sys;
extern crate libc;

pub struct CommunicationContext<'a> {
    endpoint: gex_EP_t, // thread safe handler
    team: gex_TM_t,
    entry_table: Entrytable,
    cmd_args: Vec<String>,

    message_handler: &'a mut MessageHandler<'a>,
    short_handler_index: gex_AM_Index_t,
    medium_handler_index: gex_AM_Index_t,
    long_handler_index: gex_AM_Index_t,
    message_recv_reply_index: gex_AM_Index_t,

    segment_len: usize,
    endpoints_data: Vec<EndpointData>,

    max_global_long_request_len: usize,
    max_global_medium_request_len: usize,

    local_rank: Rank,
    world_size: usize,

    notifiers: Vec<mpsc::SyncSender<usize>>, // notify message sender all message fragment is sent
    message_framgment_checker: Vec<(usize, BinaryHeap<Reverse<usize>>)>, // expecting, smallest received
}

#[derive(Clone, Debug)]
struct EndpointData {
    segment_addr: *mut c_void,
    segment_len: usize,
}

#[derive(Copy, Clone, Debug)]
pub struct Rank {
    rank: i32,
}

impl Rank {
    pub fn new(rank: i32) -> Self {
        Rank { rank }
    }
    pub fn as_i32(&self) -> i32 {
        self.rank
    }
    pub fn as_usize(&self) -> usize {
        self.rank.try_into().unwrap()
    }
    fn gex_rank(&self) -> gex_Rank_t {
        self.rank.try_into().unwrap()
    }
    fn from_gex_rank(rank: gex_Rank_t) -> Self {
        Self::new(rank.try_into().unwrap())
    }
}
impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rank)
    }
}

union B128 {
    u64_2: (u64, u64),
    i32_4: (i32, i32, i32, i32),
}

fn u64_2_to_i32_4(a: u64, b: u64) -> (i32, i32, i32, i32) {
    unsafe { B128 { u64_2: (a, b) }.i32_4 }
}

fn i32_4_to_u64_2(a: i32, b: i32, c: i32, d: i32) -> (u64, u64) {
    unsafe {
        B128 {
            i32_4: (a, b, c, d),
        }
        .u64_2
    }
}

const COM_CONTEXT_NULL: *mut CommunicationContext = null_mut::<CommunicationContext>();
static mut GLOBAL_CONTEXT_PTR: *mut CommunicationContext = COM_CONTEXT_NULL;

extern "C" fn recv_short(token: gex_Token_t, arg0: gasnet_handlerarg_t) {
    let t_info = gex_token_info(token);
    println!("receive from {}, value {}", t_info.gex_srcrank, arg0);
}

extern "C" fn recv_medium(token: gex_Token_t, buf: *const c_void, nbytes: size_t) {
    let t_info = gex_token_info(token);
    let src = Rank::from_gex_rank(t_info.gex_srcrank);
    // trace!("medium recv {} bytes from {} at mem {:?}", nbytes, src, buf);

    unsafe {
        let buf = slice::from_raw_parts(buf as *const u8, nbytes.try_into().unwrap());
        debug_assert!(
            GLOBAL_CONTEXT_PTR != COM_CONTEXT_NULL,
            "Context pointer is null"
        );
        ((*GLOBAL_CONTEXT_PTR).message_handler)(src, buf);
    }
}

fn _recv_long<const TEST_ON: usize>(
    // TODO: change TEST_ON to a type
    token: gex_Token_t,
    buf: *const c_void,
    nbytes: size_t,
    a: gasnet_handlerarg_t,
    b: gasnet_handlerarg_t,
    c: gasnet_handlerarg_t,
    d: gasnet_handlerarg_t,
) {
    let src = if TEST_ON == 0 {
        let t_info = gex_token_info(token);
        Rank::from_gex_rank(t_info.gex_srcrank)
    } else {
        Rank::new(0)
    };
    let (message_len, offset) = i32_4_to_u64_2(a, b, c, d);
    let message_len: usize = message_len.try_into().unwrap();
    let offset: usize = offset.try_into().unwrap();
    let nbytes: usize = nbytes.try_into().unwrap();

    // trace!("long recv {} bytes from {} at mem {:?}", nbytes, src, buf);

    let call_handler = || unsafe {
        let buf = slice::from_raw_parts(
            buf.offset(-(TryInto::<isize>::try_into(offset).unwrap())) as *const u8,
            message_len,
        );
        debug_assert!(
            GLOBAL_CONTEXT_PTR != COM_CONTEXT_NULL,
            "Context pointer is null"
        );
        ((*GLOBAL_CONTEXT_PTR).message_handler)(src, buf);
    };

    if message_len == nbytes {
        // whole message received
        debug_assert!(offset == 0);
        call_handler();
        return; // send no reply if whole message
    }
    // now message is fragmented
    let context: &mut CommunicationContext = unsafe { &mut *GLOBAL_CONTEXT_PTR };
    let (ref mut expecting, ref mut heap) = context.message_framgment_checker[src.as_usize()];
    heap.push(std::cmp::Reverse(offset));
    loop {
        // pop until mistach or empty
        match heap.peek() {
            None => break,
            Some(Reverse(minimal)) if *minimal == *expecting => {
                *expecting += context.max_global_long_request_len;
                heap.pop();
            }
            _ => break,
        }
    }
    if *expecting >= message_len {
        debug_assert!(heap.is_empty());
        *expecting = 0;
        if TEST_ON == 0 {
            gex_am_reply_short0(token, context.message_recv_reply_index); // reply earlier
        }
        call_handler();
    }
}

/// when received reply from remote indicating the whole message is received
extern "C" fn message_recv_reply(token: gex_Token_t) {
    let t_info = gex_token_info(token);
    let src = t_info.gex_srcrank as usize;

    let context: &CommunicationContext = unsafe { &*GLOBAL_CONTEXT_PTR };

    use mpsc::TrySendError;
    match context.notifiers[src].try_send(0) {
        Ok(()) => (), // ok
        Err(TrySendError::Full(_)) => panic!("should not be full"),
        Err(TrySendError::Disconnected(_)) => panic!(),
    }
}

extern "C" fn recv_long(
    token: gex_Token_t,
    buf: *const c_void,
    nbytes: size_t,
    a: gasnet_handlerarg_t,
    b: gasnet_handlerarg_t,
    c: gasnet_handlerarg_t,
    d: gasnet_handlerarg_t,
) {
    _recv_long::<0>(token, buf, nbytes, a, b, c, d);
}
type MessageHandler<'a> = dyn 'a + for<'b> FnMut(Rank, &'b [u8]);

impl<'a> CommunicationContext<'a> {
    pub fn new(handler: &'a mut MessageHandler<'a>) -> Self {
        assert!(
            unsafe { GLOBAL_CONTEXT_PTR == COM_CONTEXT_NULL },
            "Should only one instance!"
        );

        let (args, _, ep, tm) = gex_client_init();

        // prepare entry table
        let mut tb = Entrytable::new();
        unsafe {
            tb.add_short_req(recv_short as *const (), 1, Some("justaname"));
            tb.add_medium_req(recv_medium as *const (), 0, Some("recv_medium"));
            tb.add_long_req(recv_long as *const (), 4, Some("recv_long"));
            tb.add_short_reply(
                message_recv_reply as *const (),
                0,
                Some("message_recv_reply"),
            );
        }

        // register the table
        gex_register_entries(ep, &mut tb);

        let context = CommunicationContext {
            endpoint: ep,
            team: tm,
            endpoints_data: vec![],
            entry_table: Entrytable::new(),
            cmd_args: args,
            message_handler: handler,
            segment_len: 0,
            max_global_long_request_len: 0,
            max_global_medium_request_len: 0,
            short_handler_index: tb[0].gex_index,
            medium_handler_index: tb[1].gex_index,
            long_handler_index: tb[2].gex_index,
            message_recv_reply_index: tb[3].gex_index,
            local_rank: Rank::from_gex_rank(gax_system_query_jobrank()),
            world_size: gax_system_query_jobsize() as usize,
            notifiers: vec![],
            message_framgment_checker: vec![],
        };

        logging::set_global_id(context.here().as_i32());
        context
    }

    pub fn run(&mut self) {
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
        }
        debug!("Endpoint data: {:?}", self.endpoints_data);
        // set proper ptr
        unsafe {
            // WARN: danger!
            GLOBAL_CONTEXT_PTR = std::mem::transmute::<
                &mut CommunicationContext<'a>,
                *mut CommunicationContext,
            >(self);
        }
    }
    pub fn single_sender(&mut self) -> SingleSender {
        // create a single sender
        assert_eq!(
            self.notifiers.len(),
            0,
            "The single sender should be initialized only once"
        );
        let mut sync_data = vec![];
        for _ in 0..self.world_size {
            let (sender, receiver) = mpsc::sync_channel(1);
            self.notifiers.push(sender);
            sync_data.push(SenderSync {
                notifiee: receiver,
                waiting_reply: RefCell::new(false),
            });
            self.message_framgment_checker.push((0, BinaryHeap::new()));
        }

        SingleSender {
            team: self.team,
            endpoints_data: self.endpoints_data.clone(),
            sync_data,
            max_global_long_request_len: self.max_global_long_request_len,
            max_global_medium_request_len: self.max_global_medium_request_len,
            short_handler_index: self.short_handler_index,
            medium_handler_index: self.medium_handler_index,
            long_handler_index: self.long_handler_index,
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

struct SenderSync {
    notifiee: mpsc::Receiver<usize>,
    waiting_reply: RefCell<bool>,
}

pub struct SingleSender {
    team: gex_TM_t, // use arc to make sender Send
    endpoints_data: Vec<EndpointData>,
    sync_data: Vec<SenderSync>,

    max_global_long_request_len: usize,
    max_global_medium_request_len: usize,
    short_handler_index: gex_AM_Index_t,
    medium_handler_index: gex_AM_Index_t,
    long_handler_index: gex_AM_Index_t,
}

// team is pointer, not send but we know it's ok to share team
unsafe impl Send for SingleSender {}

impl SingleSender {
    pub fn send(&self, dst: Rank, message: &[u8]) {
        // NOTE: not thread safe!
        if message.len() < self.max_global_medium_request_len {
            unsafe {
                gex_am_reqeust_medium0(
                    self.team,
                    dst.gex_rank(),
                    self.medium_handler_index,
                    message.as_ptr() as *const c_void,
                    message.len() as size_t,
                    gex_event_now(), // block
                )
            };
        } else {
            let chunk_size = self.endpoints_data[dst.as_usize()].segment_len;
            assert!(
                // TODO: support larger message
                message.len() < chunk_size,
                "Current impl limits msg.len:{} < chunk_size:{}",
                message.len(),
                chunk_size
            );

            let dst_index = dst.as_usize();
            let mut wait_ref = self.sync_data[dst_index].waiting_reply.borrow_mut();
            if *wait_ref {
                // block on channel
                use mpsc::TryRecvError;
                loop {
                    match self.sync_data[dst_index].notifiee.try_recv() {
                        // TODO: spin here need to rewrite.
                        Err(TryRecvError::Disconnected) => {
                            panic! {"should never disconnect when I am waiting a reply"}
                        }
                        Err(TryRecvError::Empty) => break,
                        Ok(_) => (), // ok
                    }
                }
            }

            let mut offset: usize = 0;
            let packet_size = self.max_global_long_request_len;
            *wait_ref = message.len() > packet_size;

            while offset < message.len() {
                let (a, b, c, d) = u64_2_to_i32_4(message.len() as u64, offset as u64);
                let send_slice = &message[offset..];
                let send_size = usize::min(send_slice.len(), packet_size);
                unsafe {
                    gex_am_reqeust_long4(
                        self.team,
                        dst.gex_rank(),
                        self.long_handler_index,
                        send_slice.as_ptr() as *const c_void,
                        send_size as size_t,
                        self.endpoints_data[dst.as_usize()].segment_addr,
                        offset.try_into().unwrap(),
                        gex_event_group(), // non block
                        a,
                        b,
                        c,
                        d,
                    )
                };
                // trace!("send offset {} bytes {}", offset, send_size);
                offset += send_size;
            }
            gex_nbi_wait_am_lc(); // wait until message buffer can be safely released
        }
    }

    pub fn barrier(&self) {
        // only support global barrier now
        let event = gex_coll_barrier_nb(self.team);
        gex_event_wait(event);
    }
}

impl<'a> Drop for CommunicationContext<'a> {
    fn drop(&mut self) {
        unsafe {
            GLOBAL_CONTEXT_PTR = COM_CONTEXT_NULL;
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use rand::prelude::*;
    use std::ptr::null;
    use std::sync::atomic;
    use std::sync::atomic::Ordering;

    fn fake_context<'a>(f: &'a mut MessageHandler) -> CommunicationContext<'a> {
        CommunicationContext {
            endpoint: null::<*const ()>() as gex_EP_t,
            team: null::<*const ()>() as gex_TM_t,
            endpoints_data: vec![],
            entry_table: Entrytable::new(),
            cmd_args: vec![],
            message_handler: f,
            segment_len: 0,
            max_global_long_request_len: 1024,
            max_global_medium_request_len: 512,
            short_handler_index: 0,
            medium_handler_index: 0,
            long_handler_index: 0,
            message_recv_reply_index: 0,
            local_rank: Rank::new(0),
            world_size: 0,
            notifiers: vec![],
            message_framgment_checker: vec![(0, BinaryHeap::new())],
        }
    }
    fn set_ptr(ctx: &mut CommunicationContext) {
        unsafe {
            GLOBAL_CONTEXT_PTR =
                std::mem::transmute::<&mut CommunicationContext, *mut CommunicationContext>(ctx)
        }
    }

    #[test]
    fn test_long_receiver() {
        let called = atomic::AtomicBool::new(false);
        let mut callback = |_a: Rank, _b: &[u8]| {
            called.store(true, Ordering::Relaxed);
        };
        let mut ctx = fake_context(&mut callback);
        set_ptr(&mut ctx);

        let message_len = 102400usize;
        let step = 1024;
        let mem_ptr: *const c_void = unsafe { null::<c_void>().offset(0x12345678) };
        let mut calls = vec![];
        let mut offset: usize = 0;
        while offset < message_len {
            let (a, b, c, d) = u64_2_to_i32_4(message_len as u64, offset as u64);
            let buf = unsafe { mem_ptr.offset(offset as isize) };
            let nbytes = usize::min(message_len - offset, step) as size_t;
            calls.push((buf, nbytes, a, b, c, d));
            offset += nbytes as usize;
        }
        type MessageOrder = Vec<(*const c_void, size_t, i32, i32, i32, i32)>;

        let test_message_order = |calls: &MessageOrder| {
            called.store(false, Ordering::Relaxed);
            for (buf, nbytes, a, b, c, d) in calls.clone().into_iter().take(calls.len() - 1) {
                _recv_long::<1>(null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d);
                assert_eq! {called.load(Ordering::Relaxed), false};
            }
            let (buf, nbytes, a, b, c, d) = calls[calls.len() - 1].clone();
            _recv_long::<1>(null::<*const ()> as gex_Token_t, buf, nbytes, a, b, c, d);
            assert_eq! {called.load(Ordering::Relaxed), true};
        };
        test_message_order(&calls);
        let reversed: MessageOrder = calls.iter().rev().cloned().collect();
        test_message_order(&reversed);
        let mut rng = rand::thread_rng();
        for i in 0..4000 {
            calls.shuffle(&mut rng);
            test_message_order(&calls);
        }
    }
}
