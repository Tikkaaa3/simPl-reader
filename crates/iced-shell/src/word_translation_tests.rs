use super::*;

fn reader() -> Reader {
    let mut source = super::super::tests::book("dictionary-test");
    let book = Arc::get_mut(&mut source).unwrap();
    book.items[0] = Item::Paragraph {
        id: "paragraph-0".into(),
        text: "Read a book. Café, don't, 日本語 and 책.".into(),
        base_direction: BaseDirection::Ltr,
        style_runs: Vec::new(),
    };
    let mut reader = Reader {
        book: Some(source),
        ..Default::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    reader
}

fn endpoint(byte_offset: usize) -> Endpoint {
    Endpoint {
        item_id: "paragraph-0".into(),
        byte_offset,
    }
}

fn select(reader: &mut Reader, from: usize, to: usize) {
    reader.selection.begin(endpoint(from));
    reader.selection.extend(endpoint(to));
    reader.selection.end_drag();
}

#[test]
fn double_click_selects_a_word_and_movement_inside_it_keeps_the_selection() {
    let mut reader = reader();
    reader.pointer = iced::Point::new(200.0, 250.0);
    let _ = update_inner(&mut reader, Message::SelectStart(endpoint(9)));
    reader.selection.end_drag();
    let _ = update_inner(&mut reader, Message::SelectStart(endpoint(9)));
    assert_eq!(reader.short_selection().as_deref(), Some("book"));
    let _ = update_inner(&mut reader, Message::SelectMove(endpoint(8)));
    assert_eq!(reader.short_selection().as_deref(), Some("book"));
    let _ = update_inner(&mut reader, Message::SelectMove(endpoint(17)));
    assert_ne!(reader.short_selection().as_deref(), Some("book"));
    assert!(reader.word_translation.word.is_none());
}

#[test]
fn automatic_release_and_manual_context_menu_share_the_same_card() {
    let mut reader = reader();
    select(&mut reader, 7, 11);
    let _ = reader.selection_released(true);
    let generation = reader.word_translation.generation;
    assert_eq!(
        reader.word_translation.popup.as_ref().unwrap().query,
        "book"
    );
    let result = dictionary::lookup("book", Language::English, Language::Turkish);
    assert!(result.as_ref().unwrap().is_some());
    let _ = update_inner(&mut reader, Message::DictionaryReady { generation, result });
    assert!(
        reader
            .dictionary_controls()
            .contains(&Control::Dictionary(Focus::Copy))
    );
    reader.word_translation.dismiss();
    reader.word_translation.settings.automatic = false;
    let _ = reader.selection_released(true);
    assert!(reader.word_translation.popup.is_none());
    assert!(
        reader
            .notes
            .popup
            .as_ref()
            .unwrap()
            .entries()
            .contains(&notes::PopupItem::Translate)
    );
    let _ = reader.notes_action(notes::Action::Popup(notes::PopupItem::Translate));
    assert!(reader.notes.popup.is_none());
    assert_eq!(
        reader.word_translation.popup.as_ref().unwrap().query,
        "book"
    );
}

#[test]
fn manual_lookup_prefers_the_selected_word_inside_an_existing_highlight() {
    let mut reader = reader();
    reader.word_translation.settings.automatic = false;
    reader.notes.marks.push(notes::Mark {
        id: 1,
        color: reader_document::annotations::HighlightColor::Yellow,
        note: false,
        bounds: selection::SelectionBounds {
            start_item: 0,
            start_byte: 0,
            end_item: 0,
            end_byte: 12,
        },
    });
    select(&mut reader, 7, 11);
    reader.context_at(endpoint(9));
    assert_eq!(
        reader.notes.popup.as_ref().unwrap().target,
        notes::Target::Selection
    );
    let _ = reader.notes_action(notes::Action::Popup(notes::PopupItem::Translate));
    assert_eq!(
        reader.word_translation.popup.as_ref().unwrap().query,
        "book"
    );
    reader.selection.clear();
    reader.context_at(endpoint(9));
    assert_eq!(
        reader.notes.popup.as_ref().unwrap().target,
        notes::Target::Highlight(1)
    );
}

#[test]
fn long_selections_use_normal_tools_and_late_results_never_reopen_cards() {
    let mut reader = reader();
    let end = reader.book.as_ref().unwrap().items[0].text().unwrap().len();
    select(&mut reader, 0, end);
    let _ = reader.selection_released(true);
    assert!(reader.word_translation.popup.is_none());
    assert!(reader.notes.popup.is_some());
    reader.notes.popup = None;
    select(&mut reader, 7, 11);
    let _ = reader.translate_selection();
    let old = reader.word_translation.generation;
    let _ = reader.translate_text("café".into());
    reader.request = 20;
    let _ = update_inner(
        &mut reader,
        Message::Loaded {
            request: 19,
            result: Err("superseded".into()),
        },
    );
    assert!(reader.word_translation.popup.is_some());
    reader.dictionary_ready(old, Ok(None));
    assert!(
        reader
            .word_translation
            .popup
            .as_ref()
            .unwrap()
            .result
            .is_none()
    );
    let current = reader.word_translation.generation;
    reader.word_translation.dismiss();
    reader.dictionary_ready(current, Ok(None));
    assert!(reader.word_translation.popup.is_none());
}

#[test]
fn language_controls_only_offer_installed_pairs_and_keep_settings_valid() {
    let mut reader = reader();
    reader.show_settings = true;
    let _ = reader.dictionary_action(Action::Picker(Picker::Source));
    for language in Language::ALL {
        assert!(reader.controls().any(
            |control| control == Control::Dictionary(Focus::Language(Picker::Source, language))
        ));
    }
    let _ = reader.dictionary_action(Action::Language(Picker::Source, Language::Korean));
    assert_eq!(reader.word_translation.settings.target, Language::English);
    let _ = reader.dictionary_action(Action::Picker(Picker::Target));
    let choices = reader.dictionary_controls();
    assert!(choices.contains(&Control::Dictionary(Focus::Language(
        Picker::Target,
        Language::English
    ))));
    assert!(!choices.contains(&Control::Dictionary(Focus::Language(
        Picker::Target,
        Language::Turkish
    ))));
    let _ = reader.dictionary_action(Action::ToggleAutomatic);
    assert!(!reader.word_translation.settings.automatic);
    assert!(reader.word_translation.picker.is_none());
}

#[test]
fn word_boundaries_are_utf8_safe_and_keep_apostrophes() {
    for (text, byte, expected) in [
        ("A café.", 5, "café"),
        ("don't stop", 3, "don't"),
        ("abc 책", 5, "책"),
        ("本", 1, "本"),
    ] {
        let range = word_range(text, byte).unwrap();
        assert_eq!(&text[range], expected);
    }
    assert_eq!(word_range("hello world", 5), Some(0..5));
    assert_eq!(word_range("hello world", 6), Some(6..11));
    assert_eq!(word_range("日本", 3), Some(3..6));
    assert!(word_range("hello, world", 6).is_none());
}

#[test]
fn cards_stay_inside_small_windows_and_escape_preserves_the_selection() {
    let mut reader = reader();
    reader.window_size = Size::new(540.0, 360.0);
    reader.pointer = iced::Point::new(535.0, 355.0);
    select(&mut reader, 7, 11);
    let _ = reader.translate_selection();
    let bounds = reader.word_translation.bounds(reader.window_size).unwrap();
    assert!(bounds.x >= 0.0 && bounds.y >= 0.0);
    assert!(bounds.x + bounds.width <= 540.0 && bounds.y + bounds.height <= 360.0);
    let event = iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(key::Named::Escape),
        modified_key: Key::Named(key::Named::Escape),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    });
    let _ = update_inner(&mut reader, Message::Event(event, window::Id::unique()));
    assert!(reader.word_translation.popup.is_none());
    assert_eq!(reader.short_selection().as_deref(), Some("book"));
}

#[test]
fn loading_saved_languages_cancels_a_lookup_started_with_startup_defaults() {
    let mut reader = reader();
    select(&mut reader, 7, 11);
    let _ = reader.translate_selection();
    let generation = reader.word_translation.generation;
    let preferences = Preferences {
        dictionary: Settings {
            automatic: false,
            source: Language::German,
            target: Language::English,
        },
        ..Default::default()
    };
    let _ = update(&mut reader, Message::PreferencesLoaded(Ok(preferences)));
    reader.dictionary_ready(generation, Ok(None));
    assert!(reader.word_translation.popup.is_none());
    assert_eq!(reader.word_translation.settings.source, Language::German);
    assert!(!reader.word_translation.settings.automatic);
}

#[test]
fn pdf_extraction_respects_length_permission_and_document_generation() {
    use crate::pdf_reader::tests::{complete, tall_pdf};
    if !std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("pdfium.dll")
        .exists()
    {
        eprintln!("skipped: pdfium.dll is not beside the test executable");
        return;
    }
    let path =
        std::env::temp_dir().join(format!("simpl-dictionary-pdf-{}.pdf", std::process::id()));
    std::fs::write(&path, tall_pdf()).unwrap();
    let mut document = complete(reader_pdf::open(path.clone())).unwrap();
    Arc::get_mut(&mut document).unwrap().can_copy = false;
    let (pdf, _) = pdf_reader::Reader::new(document.clone(), None, Size::new(1280.0, 800.0), 1.0);
    let mut reader = Reader {
        pdf: Some(pdf),
        ..Default::default()
    };
    let _ = reader.pdf.as_mut().unwrap().show_match(0, 8, 17);
    assert!(reader.auto_translate_selection().is_none());
    let _ = reader.translate_selection();
    assert!(matches!(
        reader.word_translation.popup.as_ref().unwrap().result,
        Some(Err(_))
    ));
    reader.pdf = None;
    Arc::get_mut(&mut document).unwrap().can_copy = true;
    let (pdf, _) = pdf_reader::Reader::new(document.clone(), None, Size::new(1280.0, 800.0), 1.0);
    reader.pdf = Some(pdf);
    let _ = reader.pdf.as_mut().unwrap().show_match(0, 8, 17);
    let _ = reader.auto_translate_selection().unwrap();
    let generation = reader.word_translation.generation;
    let _ = reader.dictionary_text(document.id + 1, generation, Ok("wrong".into()));
    assert_eq!(
        reader.word_translation.popup.as_ref().unwrap().query,
        "Selected word"
    );
    let text = complete(
        document
            .session
            .copy(reader.pdf.as_ref().unwrap().selection().unwrap()),
    )
    .unwrap();
    assert_eq!(text, "Lighthouse");
    let _ = reader.dictionary_text(document.id, generation, Ok(text));
    assert_eq!(
        reader.word_translation.popup.as_ref().unwrap().query,
        "Lighthouse"
    );
    let _ = reader.dictionary_text(document.id, generation, Ok("wrong".into()));
    assert_eq!(
        reader.word_translation.popup.as_ref().unwrap().query,
        "Lighthouse"
    );
    let _ = reader.auto_translate_selection().unwrap();
    let generation = reader.word_translation.generation;
    let _ = reader.dictionary_text(
        document.id,
        generation,
        Ok("one two three four five".into()),
    );
    assert!(reader.word_translation.popup.is_none());
    assert!(reader.notes.popup.is_some());
    drop(reader);
    drop(document);
    let _ = std::fs::remove_file(path);
}

#[test]
#[ignore = "Visual dictionary QA: renders production widgets to target/dictionary-previews"]
fn render_dictionary_previews() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from("../../target/dictionary-previews");
    std::fs::create_dir_all(&output).unwrap();
    let mut reader = reader();
    for (name, size) in [
        ("wide", Size::new(1280.0, 800.0)),
        ("narrow", Size::new(540.0, 800.0)),
        ("small", Size::new(540.0, 360.0)),
    ] {
        reader.window_size = size;
        reader.pointer = iced::Point::new(size.width - 50.0, size.height - 50.0);
        for (tone, appearance) in [("light", Appearance::Light), ("dark", Appearance::Dark)] {
            reader.appearance = appearance;
            reader.show_settings = false;
            select(&mut reader, 7, 11);
            let _ = reader.translate_selection();
            reader.dictionary_ready(
                reader.word_translation.generation,
                dictionary::lookup("book", Language::English, Language::Turkish),
            );
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("card-{name}-{tone}.png")),
            );
            reader.word_translation.dismiss();
            reader.show_settings = true;
            reader.word_translation.picker = Some(Picker::Source);
            reader.focused = Some(Control::Dictionary(Focus::Language(
                Picker::Source,
                Language::Korean,
            )));
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("settings-{name}-{tone}.png")),
            );
        }
    }
}
