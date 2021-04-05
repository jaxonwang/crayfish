use gex_sys::*;
use std::boxed::Box;
use std::os::raw::*;
use std::ptr::null;

extern crate gex_sys;

// short
extern "C" fn handlers(token: gex_Token_t, arg0: gasnet_handler_t) {
    let srcrank = gex_token_info(token);
    println!("receive from {}, value {}", srcrank, arg0);
}

struct Entrytable {
    entries: Vec<gex_AM_Entry_t>,
}

impl Entrytable {
    pub fn new() -> Self {
        Entrytable { entries: vec![] }
    }

    pub fn add<F>(&mut self, f: F, flags: gex_Flags_t, nargs: usize, name: Option<&'static str>) {
        let handler = unsafe { std::mem::transmute::<F, extern "c" fn()>(f) };
        let entry = gex_AM_Entry_t {
            gex_index: 0,
            gex_fnptr: Some(handler),
            gex_flags: flags,
            gex_nargs: nargs as c_uint,
            gex_cdata: null::<c_void>(),
            gex_name: match name {
                None => null::<c_char>(),
                Some(s) => name.as_ptr() as *const c_char,
            },
        };
        self.entries.push(entry);
    }

    pub fn add_short_req<F>(&mut self, f: f, nargs: usize, name: Option<&'static str>) {
        self.add(f, GEX_FLAG_AM_SHORT | GEX_FLAG_AM_REQUEST, nargs, name);
    }

    pub fn add_medium_req<F>(&mut self, f: f, nargs: usize, name: Option<&'static str>) {
        self.add(f, GEX_FLAG_AM_MEDIUM | GEX_FLAG_AM_REQUEST, nargs, name);
    }

    pub fn add_long_req<F>(&mut self, f: f, nargs: usize, name: Option<&'static str>) {
        self.add(f, GEX_FLAG_AM_LONG | GEX_FLAG_AM_REQUEST, nargs, name);
    }
}

struct CommunicationContext {
    endpoint: gex_EP_t, // thread safe handler
    endpoints_data: Box<Vec<EndpointData>>,
    cmd_args: Vec<String>,
}

struct EndpointData {
    segment_addr: *const u8,
    segment_len: usize,
    max_request_len: usize,
}

struct Place {}

struct PlaceGroup {}

impl CommunicationContext {
    pub fn new() -> Self {
        let (args, _, ep, tm) = gex_client_init();

        // prepare entry table
        let mut tb = Entrytable::new();
        tb.add_short_req(handlers, 1, Some("justaname"));

        // register the table
        assert_gasnet_ok(unsafe { gex_EP_RegisterHandlers_Wrap(ep, tb.as_mut_ptr(), tb.len()) });

        // init the segment
        let seg_len: usize = 2 * 1024 * 1024 * 1024; // 2 GB
        let max_seg_len = gasnet_getMaxLocalSegmentSize_Wrap() as usize;
        let seg_len = usize::max(seg_len, max_seg_len);
        let seg = gex_segment_attach(tm, seg_len);

    }
}
