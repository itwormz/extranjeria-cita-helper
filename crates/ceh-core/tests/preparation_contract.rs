use ceh_core::{preparation::{SessionPreparation,PreparationDecision},model::*};
fn page(id:&str,kind:PageKind,method:RequestMethod)->Observation{Observation{document:DocumentId::new(id).unwrap(),kind,method,operation_code:false,variant:String::new()}}
#[test]
fn preparation_requires_two_waits_and_exactly_one_get_reload(){
 let first=page("first",PageKind::Province,RequestMethod::Get);let second=page("second",PageKind::Province,RequestMethod::Get);
 let mut prep=SessionPreparation::new(5000,8000,45000).unwrap();
 assert_eq!(prep.observe(&first,true,0).unwrap(),PreparationDecision::WaitUntil(5000));
 assert_eq!(prep.observe(&first,true,4999).unwrap(),PreparationDecision::WaitUntil(5000));
 assert_eq!(prep.observe(&first,true,5000).unwrap(),PreparationDecision::ReloadGet);
 prep.reload_sent(DispatchOutcome::Unknown,5001).unwrap();
 assert_eq!(prep.observe(&first,true,5002).unwrap(),PreparationDecision::WaitUntil(5502));
 assert_eq!(prep.observe(&second,true,6000).unwrap(),PreparationDecision::WaitUntil(14000));
 assert_eq!(prep.observe(&second,true,14000).unwrap(),PreparationDecision::Continue);
 assert_eq!(prep.observe(&second,true,20000).unwrap(),PreparationDecision::Continue);
}
#[test]
fn pause_shifts_wait_and_changed_document_never_receives_an_old_reload(){
 let first=page("first",PageKind::Province,RequestMethod::Get);let newer=page("new",PageKind::Province,RequestMethod::Get);
 let mut prep=SessionPreparation::new(5000,5000,45000).unwrap();prep.observe(&first,true,0).unwrap();prep.pause(1000).unwrap();
 assert!(prep.observe(&first,true,2000).is_err());prep.resume(11000).unwrap();
 assert_eq!(prep.observe(&first,true,12000).unwrap(),PreparationDecision::WaitUntil(15000));
 assert_eq!(prep.observe(&newer,true,15000).unwrap(),PreparationDecision::WaitUntil(20000));
 assert!(prep.observe(&first,true,14000).is_err());
}
#[test]
fn booking_bypasses_preparation_and_post_or_provider_pages_are_never_reloaded(){
 for kind in [PageKind::Booking,PageKind::AuthWait,PageKind::Applicant,PageKind::Office,PageKind::Blocked]{
  let mut prep=SessionPreparation::new(5000,5000,45000).unwrap();prep.observe(&page("first",PageKind::Province,RequestMethod::Get),true,0).unwrap();
  assert_eq!(prep.observe(&page("manual",kind,RequestMethod::Post),true,6000).unwrap(),PreparationDecision::Continue);
 }
 let mut prep=SessionPreparation::new(5000,5000,45000).unwrap();assert_eq!(prep.observe(&page("post",PageKind::Province,RequestMethod::Post),true,0).unwrap(),PreparationDecision::Continue);
 let mut prep=SessionPreparation::new(5000,5000,45000).unwrap();assert_eq!(prep.observe(&page("foreign",PageKind::Province,RequestMethod::Get),false,0).unwrap(),PreparationDecision::Continue);
}
#[test]
fn missing_reload_completion_holds_without_replaying_and_overflow_is_rejected(){
 let page=page("first",PageKind::Province,RequestMethod::Get);let mut prep=SessionPreparation::new(1,1,10).unwrap();prep.observe(&page,true,0).unwrap();prep.observe(&page,true,1).unwrap();prep.reload_sent(DispatchOutcome::Dispatched,1).unwrap();
 assert_eq!(prep.observe(&page,true,11).unwrap(),PreparationDecision::Hold);
 assert_eq!(prep.observe(&page,true,12).unwrap(),PreparationDecision::Hold);
 assert!(SessionPreparation::new(1,1,0).is_err());
 let mut prep=SessionPreparation::new(5,5,10).unwrap();assert!(prep.observe(&page,true,u64::MAX).is_err());
}

#[test]
fn each_wait_extends_while_the_network_is_busy_but_never_beyond_ten_seconds() {
 let first=page("first",PageKind::Province,RequestMethod::Get);let second=page("second",PageKind::Province,RequestMethod::Get);
 let mut prep=SessionPreparation::new(5000,8000,45000).unwrap();
 assert_eq!(prep.observe_quiet(&first,true,0,false).unwrap(),PreparationDecision::WaitUntil(5000));
 // The pause is over but requests are still in flight: keep waiting in short steps.
 assert_eq!(prep.observe_quiet(&first,true,5000,false).unwrap(),PreparationDecision::WaitUntil(5250));
 assert_eq!(prep.observe_quiet(&first,true,9000,false).unwrap(),PreparationDecision::WaitUntil(9250));
 // Quiet again: the reload goes out.
 assert_eq!(prep.observe_quiet(&first,true,9250,true).unwrap(),PreparationDecision::ReloadGet);
 prep.reload_sent(DispatchOutcome::Unknown,9300).unwrap();
 assert_eq!(prep.observe_quiet(&second,true,9400,true).unwrap(),PreparationDecision::WaitUntil(17400));
 // Second wait: busy up to the ten second budget, then continue regardless (Python ignores the timeout).
 assert_eq!(prep.observe_quiet(&second,true,17400,false).unwrap(),PreparationDecision::WaitUntil(17650));
 assert_eq!(prep.observe_quiet(&second,true,27399,false).unwrap(),PreparationDecision::WaitUntil(27649));
 assert_eq!(prep.observe_quiet(&second,true,27400,false).unwrap(),PreparationDecision::Continue);
}
#[test]
fn a_busy_network_budget_is_spent_once_per_wait_and_shifts_with_a_pause() {
 let first=page("first",PageKind::Province,RequestMethod::Get);
 let mut prep=SessionPreparation::new(1000,1000,45000).unwrap();prep.observe_quiet(&first,true,0,true).unwrap();
 assert_eq!(prep.observe_quiet(&first,true,1000,false).unwrap(),PreparationDecision::WaitUntil(1250));
 prep.pause(2000).unwrap();prep.resume(50000).unwrap();
 // 48 s paused do not count against the ten second budget.
 assert_eq!(prep.observe_quiet(&first,true,50000,false).unwrap(),PreparationDecision::WaitUntil(50250));
 assert_eq!(prep.observe_quiet(&first,true,58999,false).unwrap(),PreparationDecision::WaitUntil(59249));
 assert_eq!(prep.observe_quiet(&first,true,59001,false).unwrap(),PreparationDecision::ReloadGet);
}
#[test]
fn observe_without_network_knowledge_behaves_as_a_quiet_network() {
 let first=page("first",PageKind::Province,RequestMethod::Get);
 let mut a=SessionPreparation::new(5000,8000,45000).unwrap();let mut b=SessionPreparation::new(5000,8000,45000).unwrap();
 for now in [0,4999,5000] {assert_eq!(a.observe(&first,true,now).unwrap(),b.observe_quiet(&first,true,now,true).unwrap());}
}
