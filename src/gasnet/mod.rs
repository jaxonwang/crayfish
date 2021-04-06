use crate::logging;
use crate::logging::*;
use gex_sys::*;
use libc::size_t;
use std::boxed::Box;
use std::os::raw::*;
use std::ptr::null;
use std::ptr::null_mut;
use std::slice;

extern crate gex_sys;
extern crate libc;

pub struct CommunicationContext {
    endpoint: gex_EP_t, // thread safe handler
    team: gex_TM_t,
    entry_table: Entrytable,
    cmd_args: Vec<String>,
    message_handler: Box<MessageHandler>,

    segment_len: usize,
    endpoints_data: Vec<EndpointData>,

    max_global_long_request_len: usize,
    max_global_medium_request_len: usize,
}

#[derive(Clone, Debug)]
struct EndpointData {
    segment_addr: *const u8,
    segment_len: usize,
}

#[derive(Copy, Clone, Debug)]
pub struct Place {
    rank: i32,
}

impl Place {
    fn new(rank: i32) -> Self {
        Place { rank }
    }
    pub fn rank(&self) -> i32 {
        self.rank
    }
}

pub struct PlaceGroup {
    size: usize, // TODO further api design
}

impl PlaceGroup {
    // pub fn iter(&self) -> placeiter{
    //     (0..self.size)
    // }
    pub fn len(&self) -> usize {
        self.size
    }
}

const COM_CONTEXT_NULL: *mut CommunicationContext = null_mut::<CommunicationContext>();
static mut GLOBAL_CONTEXT_PTR: *mut CommunicationContext = COM_CONTEXT_NULL;

extern "C" fn recv_short(token: gex_Token_t, arg0: gasnet_handler_t) {
    let t_info = gex_token_info(token);
    println!("receive from {}, value {}", t_info.gex_srcrank, arg0);
}

extern "C" fn recv_medium_long(token: gex_Token_t, buf: *const c_void, nbytes: size_t) {
    let t_info = gex_token_info(token);
    let src = Place::new(t_info.gex_srcrank as i32);

    unsafe {
        let buf = slice::from_raw_parts(buf as *const u8, nbytes);
        debug_assert!(
            GLOBAL_CONTEXT_PTR != COM_CONTEXT_NULL,
            "Context pointer is null"
        );
        ((*GLOBAL_CONTEXT_PTR).message_handler)(src, buf);
    }
}

type MessageHandler = dyn for<'a> FnMut(Place, &'a[u8]);

impl CommunicationContext {
    pub fn new(handler: Box<MessageHandler>) -> Box<Self> {
        assert!(
            unsafe{GLOBAL_CONTEXT_PTR == COM_CONTEXT_NULL},
            "Should only one instance!"
        );

        let (args, _, ep, tm) = gex_client_init();

        // prepare entry table
        let mut tb = Entrytable::new();
        tb.add_short_req(recv_short as *const (), 1, Some("justaname"));
        tb.add_medium_req(recv_medium_long as *const (), 0, Some("recv_medium_long"));
        tb.add_long_req(recv_medium_long as *const (), 0, Some("recv_medium_long"));

        // register the table
        gex_register_entries(ep, &mut tb);

        let mut context = Box::new(CommunicationContext {
            endpoint: ep,
            team: tm,
            endpoints_data: vec![],
            entry_table: Entrytable::new(),
            cmd_args: args,
            message_handler: handler,
            segment_len: 0,
            max_global_long_request_len: 0,
            max_global_medium_request_len: 0,
        });

        // set proper ptr
        let context_borrow: &mut CommunicationContext = context.as_mut();
        unsafe { // WARN: danger!
            GLOBAL_CONTEXT_PTR = context_borrow as *mut CommunicationContext;
        }

        logging::set_global_id(context.here().rank());
        context
    }

    pub fn run(&mut self) {
        // init the segment
        let seg_len: usize = 2 * 1024 * 1024 * 1024; // 2 GB
        let max_seg_len = gasnet_get_max_local_segment_size();
        let seg_len = usize::min(seg_len, max_seg_len);
        let seg = gex_segment_attach(self.team, seg_len);
        self.segment_len = gax_segment_query_size(seg);
        debug!("Setting gasnet segment length to {}", self.segment_len);

        // set max global long
        self.max_global_long_request_len = gex_am_max_global_request_long(self.team);
        self.max_global_medium_request_len = gex_am_max_global_request_medium(self.team);
        debug!(
            "Setting max global long reqeust length to {}",
            self.max_global_long_request_len
        );
        debug!(
            "Setting max global medium reqeust length to {}",
            self.max_global_medium_request_len
        );

        // set endpoint addr offset
        let world_size = gax_system_query_jobsize();
        for i in 0..world_size {
            let (segment_addr, segment_len) = gex_ep_query_bound_segment(self.team, i as gex_Rank_t);
            self.endpoints_data.push(EndpointData {
                segment_addr:segment_addr as *const u8,
                segment_len,
            });
        }
        debug!("Endpoint data: {:?}", self.endpoints_data);
    }

    pub fn cmd_args(&self) -> &[String] {
        &self.cmd_args[..]
    }

    pub fn here(&self) -> Place {
        Place::new(gax_system_query_jobrank() as i32)
    }

    pub fn world(&self) -> PlaceGroup {
        PlaceGroup {
            size: gax_system_query_jobsize() as usize,
        }
    }
}

impl Drop for CommunicationContext {
    fn drop(&mut self) {
        debug_assert!(unsafe{GLOBAL_CONTEXT_PTR != COM_CONTEXT_NULL});
        unsafe {
            GLOBAL_CONTEXT_PTR = COM_CONTEXT_NULL;
        }
    }
}
