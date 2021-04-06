use gex_sys::*;
use libc::size_t;
use std::boxed::Box;
use std::os::raw::*;
use std::ptr::null;
use std::ptr::null_mut;
use std::slice;
use std::pin::Pin;

extern crate gex_sys;
extern crate libc;

pub struct CommunicationContext<'a> {
    endpoint: gex_EP_t, // thread safe handler
    team: gex_TM_t,
    entry_table: Entrytable,
    cmd_args: Vec<String>,
    message_handler: &'a MessageHandler,

    segment_len: usize,
    endpoints_data: Vec<EndpointData>,

    max_global_long_request_len: usize,
    max_global_medium_request_len: usize,
}

#[derive(Clone)]
struct EndpointData {
    segment_addr: *const u8,
    segment_len: usize,
}

#[derive(Copy, Clone)]
pub struct Place {
    rank: i32,
}

impl Place {
    fn new(rank:i32) ->Self{
        Place{rank}
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

const COM_CONTEXT_NULL: *const CommunicationContext = null::<CommunicationContext>();
static mut global_context_ptr: *const CommunicationContext = COM_CONTEXT_NULL;

extern "C" fn recv_short(token: gex_Token_t, arg0: gasnet_handler_t) {
    let t_info = gex_token_info(token);
    println!("receive from {}, value {}", t_info.gex_srcrank, arg0);
}

extern "C" fn recv_medium_long(token: gex_Token_t, buf: *const c_void, nbytes: size_t) {
    let t_info = gex_token_info(token);
    let src = Place::new(t_info.gex_srcrank);

    unsafe{
        let buf = slice::from_raw_parts(c_void as *const u8, nbytes);
        debug_assert!(
            global_context_ptr != COM_CONTEXT_NULL,
            "Context pointer is null"
        );
        *global_context_ptr.message_handler(src, buf);
    }
}

pub trait MessageHandler {}

impl MessageHandler for FnMut(Place, &[u8]) {}

impl CommunicationContext {
    pub fn new(handler: &dyn MessageHandler) -> Pin<Box<Self>> {
        assert!(
            global_context_ptr == COM_CONTEXT_NULL,
            "Should only one instance!"
        );

        let (args, _, ep, tm) = gex_client_init();

        // prepare entry table
        let mut tb = Entrytable::new();
        tb.add_short_req(handlers as *const (), 1, Some("justaname"));
        tb.add_medium_req(recv_medium_long as *const (), 0, Some("recv_medium_long"));
        tb.add_long_req(recv_medium_long as *const (), 0, Some("recv_medium_long"));

        // register the table
        gex_register_entries(ep, &mut tb);

        let context = Box::pin(CommunicationContext {
            endpoint: ep,
            team: tm,
            endpoints_data: vec![],
            entry_table: Entrytable::new(),
            cmd_args: args,
            message_handler: handler,
            segment_len: 0,
            max_global_long_request_len:0,
            max_global_medium_request_len:0
        });

        // set proper ptr
        let context : &CommunicationContext = contex.get_ref().borrow();
        unsafe{
            global_context_ptr = context as *const CommunicationContext;
        }
        context
    }

    impl Drop for CommunicationContext{
        fn drop(&mut self){
            debug_assert!(global_context_ptr != COM_CONTEXT_NULL);
            unsafe{
                global_context_ptr = COM_CONTEXT_NULL;
            }
        }
    }

    pub fn run(&mut self){
        // init the segment
        let seg_len: usize = 2 * 1024 * 1024 * 1024; // 2 GB
        let max_seg_len = gasnet_get_max_local_segment_size();
        let seg_len = usize::min(seg_len, max_seg_len);
        let seg = gex_segment_attach(self.tm, seg_len);
        self.segment_len = gax_segment_query_size(seg);

        // set max global long 
        self.max_global_long_request_len = gex_am_max_global_request_long(self.tm);
        self.max_global_medium_request_len = gex_am_max_global_request_medium(self.tm);

        // set endpoint addr offset
        let world_size = gax_system_query_jobsize();
        for i in 0..world_size{
            let (segment_addr, segment_len) = gex_ep_query_bound_segment(self.tm, i as i32);
            self.endpoints_data.push(EndpointData{segment_addr, segment_len});
        }
    }

    pub fn cmd_args(&self) -> &[String] {
        &self.cmd_args[..]
    }

    pub fn here(&self) -> Place {
        Place::new( gax_system_query_jobrank() as i32)
    }

    pub fn world(&self) -> PlaceGroup {
        PlaceGroup {
            size: gax_system_query_jobsize() as usize,
        }
    }
}
