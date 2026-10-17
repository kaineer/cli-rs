use gtk::CssProvider;
use gtk::gdk::Display;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, FileChooserAction, FileChooserDialog, Label, ResponseType,
};
use gtk4 as gtk;
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "org.example.ClipboardLines";

fn main() -> gtk::glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

/// Загружаем CSS для стилизации лейбла с числом.
fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        r#"
        .big-number {
            font-size: 72px;
            font-weight: bold;
        }
        "#,
    );

    gtk::style_context_add_provider_for_display(
        &Display::default().expect("Не удалось получить дисплей"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_ui(app: &Application) {
    load_css();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Clipboard Lines")
        .default_width(320)
        .default_height(220)
        .build();

    // Лейбл, в котором показываем количество строк (или прочерк).
    let result_label = Label::builder()
        .label("—")
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    result_label.add_css_class("big-number");

    // Кнопка выбора каталога.
    let choose_button = gtk::Button::with_label("Выбрать каталог…");
    choose_button.set_margin_top(6);
    choose_button.set_margin_bottom(6);
    choose_button.set_margin_start(6);
    choose_button.set_margin_end(6);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
    vbox.append(&result_label);
    vbox.append(&choose_button);

    // Ячейка для хранения выбранного пути к каталогу.
    let chosen_dir: Rc<RefCell<Option<std::path::PathBuf>>> = Rc::new(RefCell::new(None));

    // Обработчик нажатия на кнопку выбора каталога.
    {
        let window = window.clone();
        let chosen_dir = chosen_dir.clone();

        choose_button.connect_clicked(move |_| {
            // Создаём GTK-шный (не нативный) диалог выбора папки.
            let dialog = FileChooserDialog::builder()
                .title("Выберите каталог")
                .transient_for(&window)
                .modal(true)
                .action(FileChooserAction::SelectFolder)
                .build();

            // Кнопки добавляем вручную — это особенность GtkFileChooserDialog.
            dialog.add_button("Отмена", ResponseType::Cancel);
            dialog.add_button("Выбрать", ResponseType::Accept);

            let chosen_dir = chosen_dir.clone();

            // Асинхронно ждём ответа пользователя.
            // run_future отдаёт управление главному циклу GTK,
            // поэтому диалог корректно отображается.
            glib::spawn_future_local(async move {
                let response = dialog.run_future().await;

                if response == ResponseType::Accept {
                    if let Some(file) = dialog.file() {
                        if let Some(path) = file.path() {
                            println!("Выбран каталог: {}", path.display());
                            *chosen_dir.borrow_mut() = Some(path);
                        }
                    }
                }

                // Уничтожаем диалог после использования.
                dialog.destroy();
            });
        });
    }

    // Буфер обмена: переиспользуем один объект между вызовами.
    let clipboard = Rc::new(RefCell::new(arboard::Clipboard::new().ok()));

    // Читаем буфер при получении окном фокуса.
    {
        let result_label = result_label.clone();
        let clipboard = clipboard.clone();

        window.connect_is_active_notify(move |win| {
            if !win.is_active() {
                return;
            }

            let text = match clipboard.borrow_mut().as_mut() {
                Some(cb) => cb.get_text().ok(),
                None => None,
            };

            let value = match text {
                Some(t) => t.lines().count(),
                None => 0,
            };

            let label = if value == 0 {
                "—".to_string()
            } else {
                value.to_string()
            };

            result_label.set_label(&label);
        });
    }

    window.set_child(Some(&vbox));

    // Первичное чтение буфера: если окно при старте уже активно,
    // сигнал is-active-notify может не сработать.
    if let Some(cb) = clipboard.borrow_mut().as_mut() {
        if let Ok(t) = cb.get_text() {
            let n = t.lines().count();
            if n > 0 {
                result_label.set_label(&n.to_string());
            }
        }
    }

    window.present();
}
