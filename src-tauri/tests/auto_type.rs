use wakenote::auto_type::AutoTypeSession;

#[test]
fn stable_partials_type_before_final_without_repeating_prefix() {
    let mut session = AutoTypeSession::default();
    session.start(1).unwrap();
    let mut output = String::new();
    session.partial(1, "안녕하세요 좋은");
    session
        .flush(false, |text| {
            output.push_str(text);
            Ok(())
        })
        .unwrap();
    assert!(output.is_empty());
    session.partial(1, "안녕하세요 좋은 하루입니다");
    session
        .flush(false, |text| {
            output.push_str(text);
            Ok(())
        })
        .unwrap();
    assert_eq!(output, "안녕하세요 ");
    session.finish(1, "안녕하세요 좋은 하루입니다.");
    session
        .flush(true, |text| {
            output.push_str(text);
            Ok(())
        })
        .unwrap();
    assert_eq!(output, "안녕하세요 좋은 하루입니다. ");
    session.finish(1, "안녕하세요 좋은 하루입니다.");
    session
        .flush(true, |text| {
            output.push_str(text);
            Ok(())
        })
        .unwrap();
    assert_eq!(output, "안녕하세요 좋은 하루입니다. ");
}

#[test]
fn later_chunk_waits_for_earlier_final_and_failures_do_not_acknowledge_input() {
    let mut session = AutoTypeSession::default();
    session.start(1).unwrap();
    session.start(2).unwrap();
    session.finish(2, "second");
    let mut output = String::new();
    session
        .flush(true, |text| {
            output.push_str(text);
            Ok(())
        })
        .unwrap();
    assert!(output.is_empty());
    session.finish(1, "first");
    assert!(
        session
            .flush(true, |_| Err("permission denied".into()))
            .is_err()
    );
    session
        .flush(true, |text| {
            output.push_str(text);
            Ok(())
        })
        .unwrap();
    assert_eq!(output, "first second ");
}

#[test]
fn disabled_or_unregistered_chunks_never_type_and_corrections_do_not_duplicate() {
    let mut session = AutoTypeSession::default();
    session.start(1).unwrap();
    session.partial(1, "one two");
    session.partial(1, "one two three");
    let mut output = String::new();
    session
        .flush(false, |text| {
            output.push_str(text);
            Ok(())
        })
        .unwrap();
    session.finish(1, "ONE revised phrase");
    assert!(
        session
            .flush(false, |text| {
                output.push_str(text);
                Ok(())
            })
            .is_err()
    );
    assert_eq!(output, "one ");
    session.start(2).unwrap();
    session.clear();
    session.finish(2, "late result");
    session.finish(99, "imported audio");
    session
        .flush(false, |_| panic!("must not type late results"))
        .unwrap();
}
