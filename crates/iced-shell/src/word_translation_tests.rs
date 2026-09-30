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
    static STORE: std::sync::LazyLock<Arc<Store>> = std::sync::LazyLock::new(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/dictionary-ui-tests-{}",
            std::process::id()
        ));
        let store = Arc::new(Store::new(root));
        let id = dictionary::package_id(Language::English, Language::Turkish).unwrap();
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/dictionaries/packs")
            .join(&dictionary::package(id).unwrap().file);
        store.import(&file, &AtomicBool::new(false)).unwrap();
        store
    });
    reader.word_translation.store = STORE.clone();
    reader.word_translation.inventory = reader.word_translation.store.inventory();
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
fn restored_profile_rejects_inventory_replies_from_before_restore() {
    let mut reader = Reader::default();
    let _ = reader.refresh_dictionaries();
    let previous_request = reader.word_translation.inventory_generation;
    let previous_store = reader.word_translation.store.clone();
    let _ = reader.profile_reply(Ok(
        super::super::profile_ui::Reply::Restored(PathBuf::new()),
    ));
    let current_request = reader.word_translation.inventory_generation;
    assert!(!Arc::ptr_eq(
        &previous_store,
        &reader.word_translation.store
    ));
    reader.dictionary_inventory(previous_request, vec![PackageState::Installed]);
    assert!(
        reader.word_translation.inventory.is_empty(),
        "a late pre-restore reply must not replace the new profile inventory"
    );
    reader.dictionary_inventory(current_request, vec![PackageState::Missing]);
    assert_eq!(
        reader.word_translation.inventory,
        vec![PackageState::Missing]
    );
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
    let result = reader
        .word_translation
        .store
        .lookup("book", Language::English, Language::Turkish);
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
fn language_controls_offer_available_pairs_and_keep_settings_valid() {
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

fn missing_reader(label: &str) -> Reader {
    let mut reader = reader();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/dictionary-ui-tests-{}-{label}",
        std::process::id()
    ));
    reader.word_translation.store = Arc::new(Store::new(path));
    reader.word_translation.inventory = reader.word_translation.store.inventory();
    reader
}

fn finish_lookup(reader: &mut Reader) {
    let popup = reader.word_translation.popup.as_ref().unwrap();
    let settings = reader.word_translation.settings;
    let result =
        reader
            .word_translation
            .store
            .lookup(&popup.query, settings.source, settings.target);
    reader.dictionary_ready(reader.word_translation.generation, result);
}

#[test]
fn missing_dictionary_shows_explicit_download_and_completion_retries_only_the_open_card() {
    let mut reader = missing_reader("completion");
    let id = dictionary::package_id(Language::English, Language::Turkish).unwrap();
    reader.word_translation.store.remove(id).unwrap();
    select(&mut reader, 7, 11);
    let _ = reader.auto_translate_selection();
    finish_lookup(&mut reader);
    assert_eq!(
        missing_package(reader.word_translation.popup.as_ref().unwrap()),
        Some(id)
    );
    assert!(
        reader
            .dictionary_controls()
            .contains(&Control::Dictionary(Focus::Download(id)))
    );
    assert!(
        reader.word_translation.package_job.is_none(),
        "automatic lookup must not start a download"
    );
    let old_lookup = reader.word_translation.generation;
    let _ = reader.dictionary_action(Action::Download(id));
    let job = reader
        .word_translation
        .package_job
        .as_ref()
        .unwrap()
        .generation;
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/dictionaries/packs")
        .join(&dictionary::package(id).unwrap().file);
    reader
        .word_translation
        .store
        .import(&file, &AtomicBool::new(false))
        .unwrap();
    let _ = reader.dictionary_package_ready(job, Ok(Some(id)));
    assert!(reader.word_translation.popup.as_ref().unwrap().automatic);
    assert!(reader.word_translation.popup.as_ref().unwrap().selection);
    assert!(
        reader
            .word_translation
            .popup
            .as_ref()
            .unwrap()
            .result
            .is_none()
    );
    reader.dictionary_ready(
        old_lookup,
        Err(LookupError::Unavailable {
            package: id,
            error: None,
        }),
    );
    assert!(
        reader
            .word_translation
            .popup
            .as_ref()
            .unwrap()
            .result
            .is_none(),
        "late missing result must not replace the retry"
    );
    finish_lookup(&mut reader);
    assert!(
        reader
            .dictionary_controls()
            .contains(&Control::Dictionary(Focus::Copy))
    );

    let _ = reader.dictionary_action(Action::Download(id));
    let job = reader
        .word_translation
        .package_job
        .as_ref()
        .unwrap()
        .generation;
    reader.word_translation.dismiss();
    let _ = reader.dictionary_package_ready(job, Ok(Some(id)));
    assert!(
        reader.word_translation.popup.is_none(),
        "completion must not reopen a dismissed card"
    );
}

#[test]
fn package_progress_cancellation_and_inventory_replies_are_generation_guarded() {
    let mut reader = missing_reader("generation");
    let id = dictionary::package_id(Language::English, Language::Turkish).unwrap();
    let _ = reader.refresh_dictionaries();
    let stale_inventory = reader.word_translation.inventory_generation;
    let _ = reader.dictionary_action(Action::Download(id));
    let generation = reader
        .word_translation
        .package_job
        .as_ref()
        .unwrap()
        .generation;
    reader.dictionary_progress(generation + 1, 100);
    assert_eq!(
        reader
            .word_translation
            .package_job
            .as_ref()
            .unwrap()
            .downloaded,
        0
    );
    reader.dictionary_progress(generation, 100);
    reader.dictionary_progress(generation, 80);
    assert_eq!(
        reader
            .word_translation
            .package_job
            .as_ref()
            .unwrap()
            .downloaded,
        100
    );
    let cancel = reader
        .word_translation
        .package_job
        .as_ref()
        .unwrap()
        .cancel
        .clone();
    let _ = reader.dictionary_action(Action::CancelDownload);
    assert!(cancel.load(Ordering::Acquire));
    let _ = reader.dictionary_action(Action::Download(PackageId(1)));
    assert_eq!(
        reader
            .word_translation
            .package_job
            .as_ref()
            .unwrap()
            .generation,
        generation,
        "new jobs wait for cancellation to finish"
    );
    let _ = reader.dictionary_package_ready(generation + 1, Ok(Some(id)));
    assert!(reader.word_translation.package_job.is_some());
    let _ = reader.dictionary_package_ready(generation, Err("Download cancelled.".into()));
    assert!(reader.word_translation.package_job.is_none());
    reader.dictionary_inventory(
        stale_inventory,
        vec![PackageState::Installed; dictionary::packages().len()],
    );
    assert!(
        reader
            .word_translation
            .inventory
            .iter()
            .all(|state| *state == PackageState::Missing)
    );
    let _ = reader.dictionary_action(Action::Download(id));
    let cancel = reader
        .word_translation
        .package_job
        .as_ref()
        .unwrap()
        .cancel
        .clone();
    reader.word_translation.package_job = None;
    assert!(
        cancel.load(Ordering::Acquire),
        "closing the reader cancels its worker"
    );
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
                reader
                    .word_translation
                    .store
                    .lookup("book", Language::English, Language::Turkish),
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

#[test]
#[ignore = "Download UI visual QA: renders all package states without using the network"]
fn render_downloadable_dictionary_previews() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/dictionary-download-previews");
    std::fs::create_dir_all(&output).unwrap();
    let id = dictionary::package_id(Language::English, Language::Turkish).unwrap();
    let mut reader = missing_reader("preview");
    reader.word_translation.store.remove(id).unwrap();
    for (name, size) in [
        ("wide", Size::new(1280.0, 800.0)),
        ("narrow", Size::new(640.0, 480.0)),
    ] {
        reader.window_size = size;
        for (tone, appearance) in [("light", Appearance::Light), ("dark", Appearance::Dark)] {
            reader.appearance = appearance;
            reader.show_settings = true;
            reader.word_translation.packages_expanded = false;
            reader.focused = Some(Control::Dictionary(Focus::Download(id)));
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("selected-{name}-{tone}.png")),
            );
            reader.word_translation.packages_expanded = true;
            for package in [id, PackageId(12)] {
                reader.focused = Some(Control::Dictionary(Focus::Download(package)));
                super::super::book_preview::render(
                    &mut reader,
                    &output.join(format!("manager-{}-{name}-{tone}.png", package.0)),
                );
            }
            reader.show_settings = false;
            reader.pointer = iced::Point::new(size.width - 60.0, size.height - 60.0);
            select(&mut reader, 7, 11);
            let _ = reader.auto_translate_selection();
            finish_lookup(&mut reader);
            reader.focused = None;
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("missing-{name}-{tone}.png")),
            );
            let _ = reader.dictionary_action(Action::Download(id));
            let generation = reader
                .word_translation
                .package_job
                .as_ref()
                .unwrap()
                .generation;
            reader.dictionary_progress(
                generation,
                dictionary::package(id).unwrap().bytes * 37 / 100,
            );
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("progress-{name}-{tone}.png")),
            );
            let _ = reader.dictionary_package_ready(
                generation,
                Err("Could not download dictionary. Check your connection and try again.".into()),
            );
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("retry-{name}-{tone}.png")),
            );
            reader.word_translation.dismiss();
            reader.word_translation.package_notice = None;
        }
    }
}

#[test]
#[ignore = "Live UI package worker QA: downloads one published dictionary into an owned test store"]
fn published_download_stream_reports_progress_and_retries_the_open_card() {
    use iced_futures::futures::{StreamExt, executor::block_on};
    use iced_runtime::{Action as RuntimeAction, task::into_stream};
    let mut reader = missing_reader("live-stream");
    let id = dictionary::package_id(Language::English, Language::Turkish).unwrap();
    select(&mut reader, 7, 11);
    let _ = reader.auto_translate_selection();
    finish_lookup(&mut reader);
    assert_eq!(
        missing_package(reader.word_translation.popup.as_ref().unwrap()),
        Some(id)
    );
    let task = reader.dictionary_action(Action::Download(id));
    let mut events = into_stream(task).unwrap();
    let mut progress = 0;
    let mut ready = false;
    block_on(async {
        while let Some(RuntimeAction::Output(message)) = events.next().await {
            match message {
                Message::DictionaryProgress { generation, bytes } => {
                    assert!(!ready);
                    assert!(bytes > 0);
                    reader.dictionary_progress(generation, bytes);
                    progress += 1;
                }
                Message::DictionaryPackageReady { generation, result } => {
                    assert_eq!(result, Ok(Some(id)));
                    assert!(progress > 0, "progress must be delivered before completion");
                    let followup = reader.dictionary_package_ready(generation, result);
                    let mut messages = into_stream(followup).unwrap();
                    while let Some(RuntimeAction::Output(message)) = messages.next().await {
                        match message {
                            Message::DictionaryInventory { generation, states } => {
                                reader.dictionary_inventory(generation, states)
                            }
                            Message::DictionaryReady { generation, result } => {
                                reader.dictionary_ready(generation, result)
                            }
                            _ => panic!("unexpected follow-up event"),
                        }
                    }
                    ready = true;
                    break;
                }
                _ => panic!("unexpected package event"),
            }
        }
    });
    assert!(ready);
    let popup = reader.word_translation.popup.as_ref().unwrap();
    assert_eq!(popup.query, "book");
    assert!(popup.automatic && popup.selection);
    assert!(matches!(popup.result, Some(Ok(Some(_)))));
    assert_eq!(
        reader.word_translation.inventory[id.0],
        PackageState::Installed
    );
    let _ = reader.word_translation.store.remove(id);
}
