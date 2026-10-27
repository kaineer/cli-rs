mod instruction;

use gtk::CssProvider;
use gtk::gdk::Display;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, FileChooserAction, FileChooserDialog,
    Label, ListBox, ListBoxRow, Orientation, ResponseType, ScrolledWindow,
};
use gtk4 as gtk;
use instruction::{InstructionSet, apply_all, apply_file, parse};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const APP_ID: &str = "org.example.ClipWatch";
const CLIPBOARD_POLL_MS: u64 = 500;

fn main() -> gtk::glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        r#"
        .status {
            font-size: 14px;
            margin: 6px;
        }
        .dir-label {
            font-size: 12px;
            opacity: 0.8;
            margin-start: 6px;
            margin-end: 6px;
        }
        .file-path {
            font-weight: bold;
        }
        .file-ops {
            font-size: 12px;
            opacity: 0.75;
        }
        .error {
            color: #c62828;
            margin: 6px;
        }
        "#,
    );

    gtk::style_context_add_provider_for_display(
        &Display::default().expect("Не удалось получить дисплей"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

struct AppState {
    chosen_dir: Option<PathBuf>,
    pending: InstructionSet,
    last_clipboard: String,
    clipboard: Option<arboard::Clipboard>,
}

fn build_ui(app: &Application) {
    load_css();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Clip Watch")
        .default_width(480)
        .default_height(420)
        .build();

    let initial_dir = std::env::current_dir().ok();
    let dir_label = Label::builder()
        .label(match &initial_dir {
            Some(path) => format!("Каталог: {}", path.display()),
            None => "Каталог не выбран".to_string(),
        })
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    dir_label.add_css_class("dir-label");

    let status_label = Label::builder()
        .label("Ожидание инструкций в буфере обмена…")
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    status_label.add_css_class("status");

    let error_label = Label::builder()
        .label("")
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    error_label.add_css_class("error");

    let choose_button = Button::with_label("Выбрать каталог…");
    let apply_all_button = Button::with_label("Применить всё");
    apply_all_button.set_sensitive(false);

    let list = ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);

    let scrolled = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .min_content_height(200)
        .child(&list)
        .build();

    let top_row = GtkBox::new(Orientation::Horizontal, 6);
    top_row.set_margin_start(6);
    top_row.set_margin_end(6);
    top_row.set_margin_top(6);
    top_row.append(&choose_button);
    top_row.append(&apply_all_button);

    let vbox = GtkBox::new(Orientation::Vertical, 4);
    vbox.append(&top_row);
    vbox.append(&dir_label);
    vbox.append(&status_label);
    vbox.append(&error_label);
    vbox.append(&scrolled);

    let state = Rc::new(RefCell::new(AppState {
        chosen_dir: initial_dir,
        pending: InstructionSet::default(),
        last_clipboard: String::new(),
        clipboard: arboard::Clipboard::new().ok(),
    }));

    // Выбор каталога.
    {
        let window = window.clone();
        let state = state.clone();
        let dir_label = dir_label.clone();
        let apply_all_button = apply_all_button.clone();
        let error_label = error_label.clone();

        choose_button.connect_clicked(move |_| {
            let dialog = FileChooserDialog::builder()
                .title("Выберите каталог")
                .transient_for(&window)
                .modal(true)
                .action(FileChooserAction::SelectFolder)
                .build();
            dialog.add_button("Отмена", ResponseType::Cancel);
            dialog.add_button("Выбрать", ResponseType::Accept);

            let state = state.clone();
            let dir_label = dir_label.clone();
            let apply_all_button = apply_all_button.clone();
            let error_label = error_label.clone();

            glib::spawn_future_local(async move {
                let response = dialog.run_future().await;
                if response == ResponseType::Accept {
                    if let Some(file) = dialog.file() {
                        if let Some(path) = file.path() {
                            let mut st = state.borrow_mut();
                            dir_label.set_label(&format!("Каталог: {}", path.display()));
                            st.chosen_dir = Some(path);
                            error_label.set_label("");
                            let can_apply =
                                st.chosen_dir.is_some() && !st.pending.is_empty();
                            apply_all_button.set_sensitive(can_apply);
                        }
                    }
                }
                dialog.destroy();
            });
        });
    }

    // Применить всё.
    {
        let state = state.clone();
        let list = list.clone();
        let status_label = status_label.clone();
        let error_label = error_label.clone();
        let apply_all_button = apply_all_button.clone();

        apply_all_button.connect_clicked(move |btn| {
            let mut st = state.borrow_mut();
            let Some(base) = st.chosen_dir.clone() else {
                error_label.set_label("Сначала выберите каталог.");
                return;
            };

            let mut applied = 0usize;
            let mut first_err: Option<String> = None;
            let mut remaining = Vec::new();
            for insn in st.pending.instructions.drain(..) {
                if first_err.is_some() {
                    remaining.push(insn);
                    continue;
                }
                let one = InstructionSet {
                    instructions: vec![insn.clone()],
                };
                match apply_all(&one, &base) {
                    Ok(()) => applied += 1,
                    Err(err) => {
                        first_err = Some(err.to_string());
                        remaining.push(insn);
                    }
                }
            }
            st.pending.instructions = remaining;

            match first_err {
                None => {
                    error_label.set_label("");
                    status_label.set_label(&format!("Применено инструкций: {applied}"));
                    btn.set_sensitive(false);
                    clear_list(&list);
                }
                Some(err) => {
                    error_label.set_label(&format!("Ошибка после {applied} инструкций: {err}"));
                    refresh_file_list(
                        &st,
                        &list,
                        &status_label,
                        &error_label,
                        btn,
                        &state,
                    );
                }
            }
        });
    }

    // Периодический опрос буфера обмена.
    {
        let state = state.clone();
        let list = list.clone();
        let status_label = status_label.clone();
        let error_label = error_label.clone();
        let apply_all_button = apply_all_button.clone();

        glib::timeout_add_local(std::time::Duration::from_millis(CLIPBOARD_POLL_MS), move || {
            poll_clipboard(
                &state,
                &list,
                &status_label,
                &error_label,
                &apply_all_button,
            );
            glib::ControlFlow::Continue
        });
    }

    // Первичное чтение.
    poll_clipboard(
        &state,
        &list,
        &status_label,
        &error_label,
        &apply_all_button,
    );

    window.set_child(Some(&vbox));
    window.present();
}

fn poll_clipboard(
    state: &Rc<RefCell<AppState>>,
    list: &ListBox,
    status_label: &Label,
    error_label: &Label,
    apply_all_button: &Button,
) {
    let text = {
        let mut st = state.borrow_mut();
        match st.clipboard.as_mut() {
            Some(cb) => cb.get_text().unwrap_or_default(),
            None => return,
        }
    };

    let mut st = state.borrow_mut();
    if text == st.last_clipboard {
        return;
    }
    st.last_clipboard = text.clone();

    let set = parse(&text);
    if set.is_empty() {
        // Нет инструкций — pending не трогаем.
        return;
    }

    st.pending = set;
    error_label.set_label("");
    refresh_file_list(&st, list, status_label, error_label, apply_all_button, state);
}

fn clear_list(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn refresh_file_list(
    st: &AppState,
    list: &ListBox,
    status_label: &Label,
    error_label: &Label,
    apply_all_button: &Button,
    state: &Rc<RefCell<AppState>>,
) {
    clear_list(list);

    let files = st.pending.files();
    let n_ins = st.pending.len();
    let n_files = files.len();

    if n_ins == 0 {
        status_label.set_label("Нет отложенных инструкций.");
        apply_all_button.set_sensitive(false);
        return;
    }

    status_label.set_label(&format!(
        "Готово к применению: {n_ins} инструкций в {n_files} файлах"
    ));
    apply_all_button.set_sensitive(st.chosen_dir.is_some());

    for path in files {
        let summaries = st.pending.summaries_for(&path);
        let row = build_file_row(
            path,
            summaries,
            state.clone(),
            list.clone(),
            status_label.clone(),
            error_label.clone(),
            apply_all_button.clone(),
        );
        list.append(&row);
    }
}

fn build_file_row(
    path: PathBuf,
    summaries: Vec<String>,
    state: Rc<RefCell<AppState>>,
    list: ListBox,
    status_label: Label,
    error_label: Label,
    apply_all_button: Button,
) -> ListBoxRow {
    let path_label = Label::builder()
        .label(path.display().to_string())
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();
    path_label.add_css_class("file-path");

    let ops_label = Label::builder()
        .label(summaries.join(" · "))
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    ops_label.add_css_class("file-ops");

    let apply_btn = Button::with_label("Применить");

    let text_col = GtkBox::new(Orientation::Vertical, 2);
    text_col.set_hexpand(true);
    text_col.append(&path_label);
    text_col.append(&ops_label);

    let row_box = GtkBox::new(Orientation::Horizontal, 8);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);
    row_box.set_margin_top(6);
    row_box.set_margin_bottom(6);
    row_box.append(&text_col);
    row_box.append(&apply_btn);

    {
        let path = path.clone();
        apply_btn.connect_clicked(move |_| {
            let result = {
                let st = state.borrow();
                let Some(base) = st.chosen_dir.clone() else {
                    error_label.set_label("Сначала выберите каталог.");
                    return;
                };
                apply_file(&st.pending, &base, &path)
            };

            match result {
                Ok(()) => {
                    let mut st = state.borrow_mut();
                    st.pending.remove_file(&path);
                    error_label.set_label("");
                    if st.pending.is_empty() {
                        status_label.set_label("Все изменения применены.");
                        apply_all_button.set_sensitive(false);
                        clear_list(&list);
                    } else {
                        refresh_file_list(
                            &st,
                            &list,
                            &status_label,
                            &error_label,
                            &apply_all_button,
                            &state,
                        );
                    }
                }
                Err(err) => {
                    error_label.set_label(&format!("Ошибка: {err}"));
                }
            }
        });
    }

    let row = ListBoxRow::new();
    row.set_child(Some(&row_box));
    row
}
