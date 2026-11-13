use ceh_core::{control::Authority, flow::FlowEffect, model::SessionId};
use ceh_desktop::{presentation::IconStatus, session_presentation::*};
fn session() -> SessionId {
    SessionId::new("presentation-test").unwrap()
}
fn frame() -> PresentationFrame {
    PresentationFrame {
        active: true,
        sound_enabled: true,
        ..Default::default()
    }
}
#[test]
fn sound_and_icon_follow_session_pause_silence_and_finish() {
    let mut authority = Authority::new(session());
    let mut presenter = SessionPresentation::new(session(), 3000);
    assert_eq!(
        presenter
            .update(&authority.snapshot(), 1, 0, frame(), &[])
            .unwrap()
            .icon,
        Some(IconStatus::Running)
    );
    let change = presenter
        .update(&authority.snapshot(), 2, 1, frame(), &[FlowEffect::Alert])
        .unwrap();
    assert_eq!(change.icon, Some(IconStatus::Attention));
    assert_eq!(change.sound, SoundCommand::Play);
    assert_eq!(
        presenter
            .update(&authority.snapshot(), 3, 2, frame(), &[])
            .unwrap()
            .sound,
        SoundCommand::None
    );
    let paused = authority.pause().unwrap();
    let change = presenter.update(&paused, 4, 3, frame(), &[]).unwrap();
    assert_eq!(change.icon, Some(IconStatus::Paused));
    assert_eq!(change.sound, SoundCommand::Stop);
    let resumed = authority.resume().unwrap();
    assert_eq!(
        presenter
            .update(&resumed, 5, 4, frame(), &[])
            .unwrap()
            .sound,
        SoundCommand::Play
    );
    let change = presenter
        .update(&resumed, 6, 5, frame(), &[FlowEffect::Silence])
        .unwrap();
    assert_eq!(change.icon, Some(IconStatus::Running));
    assert_eq!(change.sound, SoundCommand::Stop);
    let change = presenter
        .update(
            &resumed,
            7,
            6,
            PresentationFrame {
                active: false,
                ..frame()
            },
            &[],
        )
        .unwrap();
    assert_eq!(change.icon, Some(IconStatus::Idle));
    assert_eq!(change.sound, SoundCommand::None);
}
#[test]
fn stale_or_foreign_events_and_reversed_time_have_no_effect() {
    let mut authority = Authority::new(session());
    let mut presenter = SessionPresentation::new(session(), 3000);
    let old = authority.snapshot();
    let paused = authority.pause().unwrap();
    presenter.update(&paused, 2, 10, frame(), &[]).unwrap();
    assert!(
        presenter
            .update(&old, 3, 11, frame(), &[FlowEffect::Alert])
            .is_err()
    );
    assert!(
        presenter
            .update(&paused, 2, 11, frame(), &[FlowEffect::Alert])
            .is_err()
    );
    let foreign = Authority::new(SessionId::new("foreign").unwrap()).snapshot();
    assert!(
        presenter
            .update(&foreign, 3, 11, frame(), &[FlowEffect::Alert])
            .is_err()
    );
    assert!(presenter.update(&paused, 3, 9, frame(), &[]).is_err());
    assert_eq!(
        presenter
            .update(&paused, 3, 12, frame(), &[])
            .unwrap()
            .sound,
        SoundCommand::None
    );
}
#[test]
fn mute_cancels_and_unmute_does_not_replay_one_shot_certificate() {
    let a = Authority::new(session());
    let mut p = SessionPresentation::new(session(), 3000);
    let once = PresentationFrame {
        repeat_alert: false,
        ..frame()
    };
    assert_eq!(
        p.update(&a.snapshot(), 1, 0, once, &[FlowEffect::Alert])
            .unwrap()
            .sound,
        SoundCommand::Play
    );
    assert_eq!(
        p.update(
            &a.snapshot(),
            2,
            1,
            PresentationFrame {
                sound_enabled: false,
                ..once
            },
            &[]
        )
        .unwrap()
        .sound,
        SoundCommand::Stop
    );
    assert_eq!(
        p.update(&a.snapshot(), 3, 2, once, &[]).unwrap().sound,
        SoundCommand::None
    );
    assert_eq!(
        p.update(&a.snapshot(), 4, 99999, once, &[]).unwrap().sound,
        SoundCommand::None
    );
}
#[test]
fn actual_core_booking_handoff_keeps_attention_after_worker_finishes() {
    use ceh_core::{flow::FlowEngine, model::*};
    let mut flow = FlowEngine::new(session(), SearchOptions::default());
    let effects = flow
        .observe(
            Observation {
                document: DocumentId::new("booking").unwrap(),
                kind: PageKind::Booking,
                method: RequestMethod::Get,
                operation_code: false,
                variant: String::new(),
            },
            0,
        )
        .unwrap();
    let mut p = SessionPresentation::new(session(), 3000);
    let retained = PresentationFrame {
        active: false,
        browser_retained: true,
        ..frame()
    };
    let change = p.update(&flow.control(), 1, 0, retained, &effects).unwrap();
    assert_eq!(change.icon, Some(IconStatus::Attention));
    assert_eq!(change.sound, SoundCommand::Play);
    assert_eq!(
        p.update(&flow.control(), 2, 10000, retained, &[])
            .unwrap()
            .sound,
        SoundCommand::Play
    );
    assert_eq!(
        p.update(&flow.control(), 3, 10001, retained, &[])
            .unwrap()
            .sound,
        SoundCommand::None
    );
}

#[test]
fn choosing_an_office_silences_the_alarm_on_the_contact_page() {
    use ceh_core::{
        flow::{FlowEngine, PageFacts},
        model::*,
    };
    let mut flow = FlowEngine::new(session(), SearchOptions::default());
    let office = Observation {
        document: DocumentId::new("office").unwrap(),
        kind: PageKind::Office,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    };
    let effects = flow
        .observe_with_facts(
            office,
            PageFacts {
                office_count: 2,
                ..Default::default()
            },
            0,
        )
        .unwrap();
    let mut p = SessionPresentation::new(session(), 3000);
    assert_eq!(
        p.update(&flow.control(), 1, 0, frame(), &effects)
            .unwrap()
            .sound,
        SoundCommand::Play
    );
    let contact = Observation {
        document: DocumentId::new("contact").unwrap(),
        kind: PageKind::Contact,
        method: RequestMethod::Post,
        operation_code: false,
        variant: String::new(),
    };
    let effects = flow.observe(contact, 1).unwrap();
    let change = p.update(&flow.control(), 2, 1, frame(), &effects).unwrap();
    assert_eq!(change.sound, SoundCommand::Stop);
    assert_eq!(change.icon, Some(IconStatus::Running));
    assert_eq!(
        p.update(&flow.control(), 3, 10000, frame(), &[])
            .unwrap()
            .sound,
        SoundCommand::None
    );
}

#[test]
fn a_new_core_alert_rearms_after_a_previous_one_shot() {
    let a = Authority::new(session());
    let mut p = SessionPresentation::new(session(), 3000);
    let once = PresentationFrame {
        repeat_alert: false,
        ..frame()
    };
    assert_eq!(
        p.update(&a.snapshot(), 1, 0, once, &[FlowEffect::Alert])
            .unwrap()
            .sound,
        SoundCommand::Play
    );
    assert_eq!(
        p.update(&a.snapshot(), 2, 1, frame(), &[FlowEffect::Alert])
            .unwrap()
            .sound,
        SoundCommand::Play
    );
}

#[test]
fn a_new_pass_starts_a_new_authority_so_the_presenter_must_be_rebuilt_with_it() {
    let mut authority = Authority::new(session());
    let mut presenter = SessionPresentation::new(session(), 3000);
    presenter.update(&authority.pause().unwrap(), 1, 0, frame(), &[]).unwrap();
    presenter.update(&authority.resume().unwrap(), 2, 1, frame(), &[]).unwrap();
    // A new pass builds a new driver and gate (epoch 0) for the same search: the old presenter refuses it...
    let fresh = Authority::new(session());
    assert!(presenter.update(&fresh.snapshot(), 3, 2, frame(), &[]).is_err());
    // ...and a presenter created with the new pass accepts it.
    let mut rebuilt = SessionPresentation::new(session(), 3000);
    assert!(rebuilt.update(&fresh.snapshot(), 3, 2, frame(), &[]).is_ok());
}
#[test]
fn live_alert_interval_retimes_the_repeat_without_rearming_a_new_alert() {
    let authority=Authority::new(session());let mut p=SessionPresentation::new(session(),3000);
    assert_eq!(p.update(&authority.snapshot(),1,1000,frame(),&[FlowEffect::Alert]).unwrap().sound,SoundCommand::Play);
    p.set_alert_interval(10000);
    assert_eq!(p.update(&authority.snapshot(),2,4000,frame(),&[]).unwrap().sound,SoundCommand::None);
    assert_eq!(p.update(&authority.snapshot(),3,11000,frame(),&[]).unwrap().sound,SoundCommand::Play);
    p.set_alert_interval(2000);
    assert_eq!(p.update(&authority.snapshot(),4,12999,frame(),&[]).unwrap().sound,SoundCommand::None);
    assert_eq!(p.update(&authority.snapshot(),5,13000,frame(),&[]).unwrap().sound,SoundCommand::Play);
}
