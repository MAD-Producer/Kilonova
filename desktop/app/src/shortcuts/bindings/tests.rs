use super::*;
fn chord(key:Key,control:bool,shift:bool,alt:bool)->KeyChord {KeyChord::new(key,control,shift,alt)}

#[test]
fn default_catalogue_preserves_every_existing_scope_modifier_and_repeat() {
    let bindings=Bindings::default();
    bindings.validate().unwrap();
    assert!(catalogue().len()>60);
    for scope in [Scope::Application,Scope::Animation,Scope::Nodes] {
        for key in KEYS {
            for flags in 0..8 {
                let m=Modifiers {control:flags&1!=0,shift:flags&2!=0,alt:flags&4!=0};
                for repeat in [false,true] {
                    assert_eq!(bindings.resolve(scope,*key,m,repeat),super::super::resolve(scope,*key,m,repeat),"{scope:?} {key:?} {m:?} repeat={repeat}");
                }
            }
        }
    }
}
#[test]
fn conflicts_are_contextual_and_rejected_changes_never_steal_or_clear_bindings() {
    let mut bindings=Bindings::default();
    let old=bindings.clone();
    assert!(bindings.bind(Scope::Nodes,Command::Group,Some(chord(Key::K,true,false,false))).unwrap_err().contains("conflict"));
    assert_eq!(bindings,old);
    assert!(bindings.bind(Scope::Application,Command::Settings,Some(chord(Key::G,false,false,false))).unwrap_err().contains("conflict"));
    assert_eq!(bindings,old);
    // Animation P and node P cannot be active in the same focus context.
    bindings.bind(Scope::Nodes,Command::ToggleNodeGrid,Some(chord(Key::P,false,false,false))).unwrap();
    assert_eq!(bindings.resolve(Scope::Nodes,Key::P,Modifiers::default(),false),Some(Command::ToggleNodeGrid));
    assert_eq!(bindings.resolve(Scope::Animation,Key::P,Modifiers::default(),false),Some(Command::Reveal(motion_core::Property::Position)));
    assert_eq!(bindings.resolve(Scope::Nodes,Key::G,Modifiers::default(),false),None);
    assert_eq!(bindings.resolve(Scope::Nodes,Key::P,Modifiers::default(),true),None);
    bindings.reset_command(Scope::Nodes,Command::ToggleNodeGrid).unwrap();
    assert_eq!(bindings,old);
}
#[test]
fn system_text_unknown_commands_and_duplicate_overrides_are_rejected() {
    for reserved in [chord(Key::Tab,false,false,true),chord(Key::F4,false,false,true),chord(Key::Delete,true,false,true),chord(Key::Escape,false,false,false),chord(Key::A,true,false,false),chord(Key::Z,true,false,false),chord(Key::Space,true,false,false)] {
        assert!(Bindings::default().bind(Scope::Application,Command::Settings,Some(reserved)).unwrap_err().contains("reserved"));
    }
    assert!(Bindings::default().bind(Scope::Nodes,Command::Settings,None).is_err());
    let mut bindings=Bindings::default();
    bindings.bind(Scope::Nodes,Command::ToggleNodeMinimap,None).unwrap();
    assert_eq!(bindings.resolve(Scope::Nodes,Key::N,Modifiers::default(),false),None);
    let bytes=bindings.export_bytes().unwrap();
    let mut value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
    let duplicate=value["bindings"]["overrides"][0].clone();
    value["bindings"]["overrides"].as_array_mut().unwrap().push(duplicate);
    assert!(serde_json::from_value::<Preset>(value).is_err());
}
#[test]
fn preset_round_trip_and_real_invalid_files_preserve_the_effective_configuration() {
    let folder=tempfile::tempdir().unwrap();
    let path=folder.path().join("preset.json");
    let mut bindings=Bindings::default();
    bindings.bind(Scope::Nodes,Command::ToggleNodeGrid,Some(chord(Key::P,false,false,false))).unwrap();
    std::fs::write(&path,bindings.export_bytes().unwrap()).unwrap();
    assert_eq!(Bindings::import(&path).unwrap(),bindings);
    let old=bindings.clone();
    for bytes in [b"{".to_vec(),vec![b' ';MAX_PRESET_BYTES+1],br#"{"version":99,"bindings":{"overrides":[]}}"#.to_vec(),br#"{"version":1,"bindings":{"overrides":[],"foreign":true}}"#.to_vec()] {
        std::fs::write(&path,&bytes).unwrap();
        assert!(Bindings::import(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(),bytes);
        assert_eq!(bindings,old);
    }
    assert!(Bindings::import(&folder.path().join("missing.json")).is_err());
}
