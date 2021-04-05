

#![allow(dead_code)]
pub fn main() {
    


    // local async
    let a = here_async!{
    }; // => desugared as
    let a = async {};



    // at place block
    let c = at!(place, {
        // work
    });
    // should be desugar as
    let c = at_async!(place, {
        // work
    }).await;

    // at place async, b is a future
    let b = at_async!(place, 
        {
            // #expr
        }
    ); // will be desugared as
    let b = if (place == local){
        async {
            expr
        }
    } else{
        let top_frame = this_activity.get_top_frame();
        let args = arg_list; // pack & serialize all args. Can not pack as ref here, since lifetime is in the scope

        #[derive(Serialize, Deserialize)]
        struct Args_anonymous(a1, a2, a3);//...

        // TODO generating global id for each activity id = place + worker_no + time? 
        // 
        /* going to generate a global function, with an id
        fn (this_activity, args){
            this_activity.get_top_frame();
            expr
        }
        */

        // there should be a table to record the outgoint calls for each thread
        let handler = run_at(place, top_frame, function_id, args);
        handler.retrive() // return a future
    };


    finish!{

        // code1

        here_async!{ };

        // code2

        at_async!(place, { });

        finish!{};

        let c = at!(place,{});

        // code4
        
        let d = at_async!(place, {});

        let e = d.await;

    }; // => should be desugar as
    {
        global_finish_stack.push();

        // all async not bind/await to a name are moved to first. that ensures will be executed before any blocking callesk
        // Async activities bind to a name is assumed to live no longer than the scope
        // also ensure async only capture vars outside the finish
        let _i = here_async!{ }; 

        let _j = at_async!(place, async{ });

        // code1
        // code2
        finish!{};
        let c = at!(place,{}); // blocked, no change
        // code4
        
        let d = at_async!(place, {});

        let e = d.await;

        _i.await; // wait all local

        this_frame.wait_all().await; // wait all remote, this is notify by task_manager.

        global_finish_stack.pop()
    
    };

    finish!{

        here_async!{ // asyn1
            hera_async!{ // asyn2

            };
            at_async!(place, { // at_async1
                at_async!(place, {}); // at_async2
            });
        }

    } // => should be desugar as
    {
        global_finish_stack.push();
        let _i = here_async!{}; //asyn1;
        let _j = here_async!{}; //asyn2;
        let _k = at_async!{ //at_async1;
            at_async!{  // at_async2;
            }
        };
        global_finish_stack.pop();
    }
    // rule: define here_async/at_async without await on them as orphan acctivity
    // 1. all direct orphan activities in a finish block will be moved to the beginning, and then
    //    awaited at the end of the finish block.
    // 2. all orphan activities in a orphan here_async activities will be moved to the beginning of
    //    the nearest finish block, and then awaited at the end of the finish block.
    
}
