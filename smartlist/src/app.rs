use std::time::Instant;

pub const INPUT_DEBOUNCE_MS: u64 = 500;

pub struct App {
    input: String,
    buffer: String,
    buffer_selected: bool,
    script_output: Vec<String>,
    selected_index: Option<usize>,
    script_name: String,
    should_quit: bool,
    pub selected_output: Option<String>,
    pub last_input_time: Instant,
    pending_update: bool,
    last_command: String,
    is_loading: bool,
    pub debug_mode: bool,
}

impl App {
    pub fn new(script_name: String, debug_mode: bool, initial_query: String) -> Self {
        let mut app = Self {
            input: initial_query.clone(),
            buffer: String::new(),
            buffer_selected: false,
            script_output: Vec::new(),
            selected_index: None,
            script_name,
            should_quit: false,
            selected_output: None,
            last_input_time: Instant::now(),
            pending_update: !initial_query.is_empty(),
            last_command: String::new(),
            is_loading: false,
            debug_mode,
        };
        if !initial_query.is_empty() {
            app.last_input_time = Instant::now();
        }
        app
    }

    // Геттеры
    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn script_output(&self) -> &Vec<String> {
        &self.script_output
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn script_name(&self) -> &str {
        &self.script_name
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn pending_update(&self) -> bool {
        self.pending_update
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn last_command(&self) -> &str {
        &self.last_command
    }

    pub fn has_input(&self) -> bool {
        !self.input.is_empty()
    }

    pub fn has_buffer(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn is_buffer_selected(&self) -> bool {
        self.buffer_selected && self.has_buffer()
    }

    pub fn has_selection(&self) -> bool {
        self.selected_index.is_some()
    }

    pub fn has_results(&self) -> bool {
        !self.script_output.is_empty()
    }

    // Сеттеры и мутаторы
    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
    }

    pub fn set_pending_update(&mut self, pending: bool) {
        self.pending_update = pending;
    }

    pub fn select_buffer(&mut self, selected: bool) {
        self.buffer_selected = selected && self.has_buffer();
        if selected {
            self.selected_index = None; // Снимаем выделение с вариантов
        }
    }

    pub fn push_char(&mut self, c: char) {
        // Если был выбран буфер, снимаем выделение с него при начале ввода
        if self.buffer_selected {
            self.buffer_selected = false;
        }
        self.input.push(c);
        self.last_input_time = Instant::now();
        self.pending_update = true;
        if self.script_output.is_empty() {
            self.selected_index = None;
        }
    }

    pub fn pop_char(&mut self) {
        // Если был выбран буфер, снимаем выделение с него при удалении
        if self.buffer_selected {
            self.buffer_selected = false;
        }
        self.input.pop();
        self.last_input_time = Instant::now();
        self.pending_update = true;
        if self.script_output.is_empty() {
            self.selected_index = None;
        }
    }

    pub fn pop_buffer_char(&mut self) {
        self.buffer.pop();
        if self.buffer.is_empty() {
            self.buffer_selected = false;
        }
    }

    pub fn append_to_buffer(&mut self, text: &str) {
        if !self.buffer.is_empty() {
            self.buffer.push('\n');
        }
        self.buffer.push_str(text);
        // После добавления автоматически выбираем буфер
        self.buffer_selected = true;
        self.selected_index = None;
    }

    // pub fn clear_buffer(&mut self) {
    //     self.buffer.clear();
    //     self.buffer_selected = false;
    // }

    pub fn clear_selection(&mut self) {
        self.selected_index = None;
    }

    pub fn clear_results(&mut self) {
        self.script_output.clear();
        self.selected_index = None;
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn update_results(&mut self, new_output: Vec<String>, command: String) {
        // Сохраняем текущий выбранный текст, если он есть
        let selected_text = self
            .selected_index
            .and_then(|i| self.script_output.get(i).cloned());

        self.script_output = new_output;
        self.last_command = command;

        // Обновляем позицию выбранного элемента, если он все еще существует
        self.selected_index = if let Some(text) = selected_text {
            self.script_output.iter().position(|s| s == &text)
        } else {
            None
        };
    }

    pub fn select_first(&mut self) {
        if !self.script_output.is_empty() {
            self.selected_index = Some(0);
            self.buffer_selected = false; // При выборе варианта снимаем выделение с буфера
        }
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.last_input_time = Instant::now();
        self.pending_update = true;
        self.selected_index = None;
        self.buffer_selected = false;
    }

    // pub fn select_last(&mut self) {
    //     if !self.script_output.is_empty() {
    //         self.selected_index = Some(self.script_output.len() - 1);
    //         self.buffer_selected = false;
    //     }
    // }

    pub fn select_next(&mut self) {
        if self.script_output.is_empty() || self.is_loading {
            return;
        }

        match self.selected_index {
            None => {
                // Если ничего не выбрано - выбираем первый
                self.selected_index = Some(0);
                self.buffer_selected = false;
            }
            Some(i) if i < self.script_output.len() - 1 => {
                self.selected_index = Some(i + 1);
                self.buffer_selected = false;
            }
            _ => {} // На последнем элементе ничего не делаем
        }
    }

    pub fn select_previous(&mut self) {
        if self.script_output.is_empty() || self.is_loading {
            return;
        }

        match self.selected_index {
            None => {
                // Если ничего не выбрано - выбираем последний
                if !self.script_output.is_empty() {
                    self.selected_index = Some(self.script_output.len() - 1);
                    self.buffer_selected = false;
                }
            }
            Some(i) if i > 0 => {
                self.selected_index = Some(i - 1);
                self.buffer_selected = false;
            }
            _ => {} // На первом элементе ничего не делаем
        }
    }

    pub fn select_current(&mut self) -> Option<String> {
        self.selected_index
            .and_then(|i| self.script_output.get(i).cloned())
    }
}
