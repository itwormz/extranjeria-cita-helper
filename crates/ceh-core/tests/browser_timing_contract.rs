use ceh_core::timings::{AssistTimings,TimingKind,parse_timings};
#[test]
fn every_browser_uses_the_same_validated_timing_ranges(){
 let timings=parse_timings(&serde_json::json!({"bootstrap_seconds":[6,10],"action_delay_seconds":[1,3],"typing_ms":[10,30],"contact_typing_ms":[300,800],"reload_seconds":[30,60]})).unwrap();
 for (kind,low,high) in [(TimingKind::Bootstrap,6.,10.),(TimingKind::Action,1.,3.),(TimingKind::Typing,10.,30.),(TimingKind::ContactTyping,300.,800.),(TimingKind::Reload,30.,60.)]{
  assert_eq!(timings.sample(kind,0.).unwrap(),low);assert_eq!(timings.sample(kind,1.).unwrap(),high);assert_eq!(timings.sample(kind,0.5).unwrap(),(low+high)/2.);
 }
 for sample in [-0.1,1.1,f64::NAN,f64::INFINITY]{assert!(AssistTimings::default().sample(TimingKind::Action,sample).is_err());}
}
