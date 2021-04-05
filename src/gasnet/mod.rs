use gex_sys::*;
use std::boxed::Box;
use std::os::raw::*;
use std::ptr::null;

extern crate gex_sys;

// short
extern "C" fn handlers(token: gex_Token_t, arg0: gasnet_handler_t) {
    let t_info = gex_token_info(token);
    println!("receive from {}, value {}", t_info.gex_srcrank, arg0);
}

pub struct CommunicationContext {
    endpoint: gex_EP_t, // thread safe handler
    endpoints_data: Box<Vec<EndpointData>>,
    cmd_args: Vec<String>,
}

#[derive(Clone)]
struct EndpointData {
    segment_addr: *const u8,
    segment_len: usize,
    max_request_len: usize,
}

#[derive(Copy, Clone)]
pub struct Place {
    rank: i32,
}

impl Place {
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

impl CommunicationContext {
    pub fn new() -> Self {
        let (args, _, ep, tm) = gex_client_init();

        // prepare entry table
        let mut tb = Entrytable::new();
        tb.add_short_req(handlers as *const (), 1, Some("justaname"));

        // register the table
        gex_register_entries(ep, &mut tb);

        // init the segment
        let seg_len: usize = 2 * 1024 * 1024 * 1024; // 2 GB
        let max_seg_len = gasnet_get_max_local_segment_size();
        let seg_len = usize::min(seg_len, max_seg_len);
        let seg = gex_segment_attach(tm, seg_len);

        CommunicationContext {
            endpoint: ep,
            endpoints_data: Box::new(vec![]),
            cmd_args: args,
        }
    }

    pub fn cmd_args(&self) -> &[String] {
        &self.cmd_args[..]
    }

    pub fn here(&self) -> Place {
        Place {
            rank: gax_system_query_jobrank() as i32,
        }
    }

    pub fn world(&self) -> PlaceGroup {
        PlaceGroup {
            size: gax_system_query_jobsize() as usize,
        }
    }
}
