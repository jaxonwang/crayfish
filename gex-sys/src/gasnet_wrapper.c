#include <gasnet_wrapper.h>

extern gex_Rank_t gex_rank_invalid() { return GEX_RANK_INVALID; }
extern gex_Event_t gex_event_invalid() { return GEX_EVENT_INVALID; }
extern gex_Event_t gex_event_no_op() { return GEX_EVENT_NO_OP; }
extern gex_Event_t* gex_event_now() { return GEX_EVENT_NOW; }
extern gex_Event_t* gex_event_defer() { return GEX_EVENT_DEFER; }
extern gex_Event_t* gex_event_group() { return GEX_EVENT_GROUP; }
extern gex_EP_t gex_ep_invalid() { return GEX_EP_INVALID; }
extern gex_Client_t gex_client_invalid() { return GEX_CLIENT_INVALID; }
extern gex_Segment_t gex_segment_invalid() { return GEX_SEGMENT_INVALID; }
extern gex_TM_t gex_tm_invalid() { return GEX_TM_INVALID; }
extern gex_TI_t gex_ti_srcrank() { return GEX_TI_SRCRANK; }
extern gex_TI_t gex_ti_ep() { return GEX_TI_EP; }
extern gex_TI_t gex_ti_entry() { return GEX_TI_ENTRY; }
extern gex_TI_t gex_ti_is_req() { return GEX_TI_IS_REQ; }
extern gex_TI_t gex_ti_is_long() { return GEX_TI_IS_LONG; }
extern gex_TI_t gex_ti_all() { return GEX_TI_ALL; }
extern gex_EC_t gex_ec_all() { return GEX_EC_ALL; }
extern gex_EC_t gex_ec_get() { return GEX_EC_GET; }
extern gex_EC_t gex_ec_put() { return GEX_EC_PUT; }
extern gex_EC_t gex_ec_am() { return GEX_EC_AM; }
extern gex_EC_t gex_ec_lc() { return GEX_EC_LC; }

extern int gex_Client_Init_Wrapper(gex_Client_t *client_p, gex_EP_t *ep_p,
                                   gex_TM_t *tm_p, const char *clientName,
                                   int *argc, char ***argv, gex_Flags_t flags) {
  return gex_Client_Init(client_p, ep_p, tm_p, clientName, argc, argv, flags);
}
