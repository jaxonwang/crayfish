use crate::activity;
use crate::activity::ActivityId;
use crate::activity::TaskItem;
use crate::finish;
use crate::finish::FinishId;
use crate::place::Place;
use future::future::Future;
use future::future::FutureExt;

extern crate future;

pub trait ApgasContext {
    fn inherit(finish_id: FinishId) -> Self;
    fn new_frame() -> Self;
    fn spwaned(&self) -> Vec<ActivityId>;
    fn spwan(&mut self, place: Place) -> ActivityId;
    fn send(item: Box<TaskItem>);
    fn wait_single<T>(&self, wait_this: ActivityId) -> Box<dyn Future<Output = T>>;
    fn wait_all(self) -> Box<dyn Future<Output = ()>>;
}

pub struct ConcreteContext {
    sub_activities: Vec<ActivityId>,
    finish_id: FinishId,
}

impl ApgasContext for ConcreteContext {
    fn inherit(finish_id: FinishId) -> Self {
        ConcreteContext {
            sub_activities: vec![],
            finish_id,
        }
    }
    fn new_frame() -> Self {
        ConcreteContext {
            sub_activities: vec![],
            finish_id: new_global_finish_id(),
        }
    }
    fn spwaned(&self) -> Vec<ActivityId>{
        self.sub_activities.clone()
    }

    fn spwan(&mut self, place: Place) {
        s
    }
}
