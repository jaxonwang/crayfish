
#![allow(dead_code)]
mod dead{
async fn real_fn(&mut ctx: Context, a: i32, b: A, c: i32) -> usize { // macro
    0
}

// block until real function finished
async fn execute_and_send_fn0(&mut ctx: Context, my_activity_id:ActivityId,src_place: Place, a: i32, b: A, c: i32) { // macro
    let future = real_fn(ctx, a, b, c); //macro
    let result = future.catch_unwind().await;

    // prepare return message
    let mut builder = SquashBufferItemBuilder::new(fn_id, src_place, my_activity_id);
    spwaned_activities = ctx.spawned(); // get activitiy spwaned in real_fn
    builder.ret(result); // macro
    builder.sub_activities(spwaned_activities);
    let item = builder.build();
    ctx.send(item)
}


// the one executed by worker
async fn real_fn_wrap_execute_from_remote(&root_ctx: Context, item: SquashBufferItem) {
    let mut e = SquashBufferItemExtracter::new(item);
    let my_activity_id = e.activity();
    let src_place = e.place();
    let fn_id = e.fn_id();
    let finish_id = get_finish_id(my_activity_id);
    let ctx = root_ctx.new_ctx(finish_id);

    // wait until function return
    exectute_and_send_fn0(&mut ctx, my_activity_id, src_place, e.arg(), e.arg_squash(), e.arg()).await; // macro 
}

// the desugered at async
fn async_create_for_fn_id_0(ctx: &mut Context, dst_place: Place, a: i32, b: A, c: i32) { // macro
    let my_activity_id = ctx.spawn(place); // register to remote
    let fn_id: FunctionLabel = 0; // macro

    let f = ctx.create_single_finish_future(my_activity_id);
    if dst_place == here {
       let future = execute_and_send(&mut ctx, my_activity_id, a, b, c); // macro
       future.then(||f)
    } else {
        let mut builder = SquashBufferItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(a); //  macro
        builder.arg_squash(b); // macro
        builder.arg(c); //macro
        let item = builder.build();
        ctx.send(item);
    }
    f
}

// desugered finish
async fn finish() {
    let ctx = ctx.new_finish_frame();
    // ctx contains a new finish id now
    let f = async_crate_for_fn_id_0(&mut ctx, dst_place, 1, A { value: 3 }, 3);

    let ret = f.await; // if await, remove it from activity list this finish block will wait
    println!("{}", ret);
    async_crate_for_fn_id_0(&mut ctx, dst_place, 1, A { value: 3 }, 3);

    ctx.wait_all().await;
}

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
}
