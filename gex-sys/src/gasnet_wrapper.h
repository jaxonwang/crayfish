#define GASNET_PAR
#include <gasnet.h>

extern gex_Rank_t gex_rank_invalid();
extern gex_Event_t gex_event_invalid();
extern gex_Event_t gex_event_no_op();
extern gex_Event_t* gex_event_now(); 
extern gex_Event_t* gex_event_defer();
extern gex_Event_t* gex_event_group();
extern gex_EP_t gex_ep_invalid();
extern gex_Client_t gex_client_invalid();
extern gex_Segment_t gex_segment_invalid();
extern gex_TM_t gex_tm_invalid();
extern gex_TI_t gex_ti_srcrank();
extern gex_TI_t gex_ti_ep();
extern gex_TI_t gex_ti_entry();
extern gex_TI_t gex_ti_is_req();
extern gex_TI_t gex_ti_is_long();
extern gex_TI_t gex_ti_all();
extern gex_EC_t gex_ec_all();
extern gex_EC_t gex_ec_get();
extern gex_EC_t gex_ec_put();
extern gex_EC_t gex_ec_am();
extern gex_EC_t gex_ec_lc();
extern int gex_Client_Init_Wrapper(gex_Client_t *client_p, gex_EP_t *ep_p,
                                   gex_TM_t *tm_p, const char *clientName,
                                   int *argc, char ***argv, gex_Flags_t flags);

/* flags are generated correctly
extern gex_Flags_t gex_flag_rank_is_jobrank() { return GEX_FLAG_RANK_IS_JOBRANK; }

extern gex_Flags_t gex_flag_am_prepare_least_client() {
  return GEX_FLAG_AM_PREPARE_LEAST_CLIENT;
}

extern gex_Flags_t gex_flag_am_prepare_least_alloc() {
  return GEX_FLAG_AM_PREPARE_LEAST_ALLOC;
}

extern gex_Flags_t gex_flag_tm_global_scratch() { return GEX_FLAG_TM_GLOBAL_SCRATCH; }

extern gex_Flags_t gex_flag_tm_local_scratch() { return GEX_FLAG_TM_LOCAL_SCRATCH; }

extern gex_Flags_t gex_flag_tm_symmetric_scratc() {
  return GEX_FLAG_TM_SYMMETRIC_SCRATCH;
}

extern gex_Flags_t gex_flag_tm_no_scratch() { return GEX_FLAG_TM_NO_SCRATCH; }

extern gex_Flags_t gex_flag_globally_quiesc() { return GEX_FLAG_GLOBALLY_QUIESCED; }
*/
