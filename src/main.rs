use libc::{ioctl, winsize, ECHO, ICANON, STDIN_FILENO, STDOUT_FILENO, TCSAFLUSH, TIOCGWINSZ, tcgetattr, tcsetattr, termios};
use std::io::{self, Read, Write};
use std::mem;
use std::fs::File;
use std::env; // 실행 인자를 가져오기 위해 추가
use std::fs::read_to_string; // 파일 내용을 읽기 위해 추가
// --- Terminal Raw Mode Handling ---
struct RawMode {
    orig_termios: termios,
}

impl RawMode {
    fn enable() -> Self {
        unsafe {
            let mut raw: termios = mem::zeroed();
            if tcgetattr(STDIN_FILENO, &mut raw) == -1 {
                panic!("tcgetattr 실패");
            }
            let orig_termios = raw;
            raw.c_lflag &= !(ECHO | ICANON); 
            if tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw) == -1 {
                panic!("tcsetattr 실패");
            }
            RawMode { orig_termios }
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        unsafe {
            tcsetattr(STDIN_FILENO, TCSAFLUSH, &self.orig_termios);
        }
    }
}

// --- Data Structures ---
#[derive(PartialEq)]
enum Mode {
    Normal,
    Insert,
    Command,
}

struct Row {
    content: String,
}

impl Row {
    fn new(s: String) -> Self {
        Row { content: s }
    }
    fn insert_char(&mut self, at: usize, c: char) {
        if at >= self.content.len() {
            self.content.push(c);
        } else {
            self.content.insert(at, c);
        }
    }
    fn delete_char(&mut self, at: usize) {
        if at < self.content.len() {
            self.content.remove(at);
        }
    }
}

struct EditorBuffer {
    rows: Vec<Row>,
}

impl EditorBuffer {
    fn new() -> Self {
        EditorBuffer {
            rows: vec![Row::new(String::new())],
        }
    }
    fn rows_to_string(&self) -> String {
        self.rows.iter()
            .map(|r| r.content.as_str())
            .collect::<Vec<&str>>()
            .join("\n")
    }
    fn open(&mut self, filename: &str) -> io::Result<()> {
        let content = read_to_string(filename)?; // 파일을 읽어옴
        self.rows.clear(); // 기본 빈 줄 제거

        for line in content.lines() {
            self.rows.push(Row::new(line.to_string())); // 한 줄씩 버퍼에 추가
        }

        // 파일이 비어있을 경우를 대비해 최소 한 줄은 유지
        if self.rows.is_empty() {
            self.rows.push(Row::new(String::new()));
        }
        Ok(())
    }
}

struct EditorConfig {
    cx: u16,
    cy: u16,
    screen_cols: u16,
    screen_rows: u16,
    row_offset: usize,
    mode: Mode,
    buffer: EditorBuffer,
    command_buffer: String,
    status_msg: String,
    filename: Option<String>,
}

impl EditorConfig {
  fn new() -> Self {
        let (cols, rows) = get_terminal_size();
        EditorConfig {
            cx: 0,
            cy: 0,
            screen_cols: cols,
            screen_rows: rows,
            row_offset: 0, // 0번 줄부터 시작
            mode: Mode::Normal,
            buffer: EditorBuffer::new(),
            command_buffer: String::new(),
            status_msg: String::from("WELCOME! :q to quit"),
            filename: None,
        }
    }

    fn move_cursor(&mut self, key: char) {
        let row_count = self.buffer.rows.len();
        match key {
            'h' => if self.cx > 0 { self.cx -= 1 },
            'j' => if (self.cy as usize) < row_count - 1 { self.cy += 1 },
            'k' => if self.cy > 0 { self.cy -= 1 },
            'l' => {
                let cur_row_len = self.buffer.rows[self.cy as usize].content.len() as u16;
                if self.cx < cur_row_len { self.cx += 1; }
            }
            _ => {}
        }
        let cur_row_len = self.buffer.rows[self.cy as usize].content.len() as u16;
        if self.cx > cur_row_len { self.cx = cur_row_len; }
    }

    fn insert_char(&mut self, c: char) {
        self.buffer.rows[self.cy as usize].insert_char(self.cx as usize, c);
        self.cx += 1;
    }

    fn delete_char(&mut self) {
        if self.cx == 0 && self.cy == 0 { return; }
        if self.cx > 0 {
            self.buffer.rows[self.cy as usize].delete_char(self.cx as usize - 1);
            self.cx -= 1;
        } else {
            let current_row_content = self.buffer.rows.remove(self.cy as usize).content;
            self.cy -= 1;
            let prev_row = &mut self.buffer.rows[self.cy as usize];
            self.cx = prev_row.content.len() as u16;
            prev_row.content.push_str(&current_row_content);
        }
    }

   fn save(&mut self) -> io::Result<()> {
        // filename이 있으면 사용, 없으면 에러 처리
        let path = match &self.filename {
            Some(name) => name,
            None => {
                self.status_msg = "No file name! Use :w <filename> (TBD)".into();
                return Ok(());
            }
        };

        let content = self.buffer.rows_to_string();
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        self.status_msg = format!("Saved to {}", path);
        Ok(())
    } 

    fn handle_keypress(&mut self, key: char) -> bool {
        match self.mode {
            Mode::Normal => match key {
                'i' => self.mode = Mode::Insert,
                ':' => {
                    self.mode = Mode::Command;
                    self.command_buffer.clear();
                }
                'h' | 'j' | 'k' | 'l' => self.move_cursor(key),
                _ => {}
            },
            Mode::Insert => match key {
                '\x1b' => self.mode = Mode::Normal,
                '\r' | '\n' => {
                    let remaining = self.buffer.rows[self.cy as usize].content.split_off(self.cx as usize);
                    self.buffer.rows.insert(self.cy as usize + 1, Row::new(remaining));
                    self.cy += 1;
                    self.cx = 0;
                }
                '\x7f' | '\x08' => self.delete_char(),
                c if !c.is_control() => self.insert_char(c),
                _ => {}
            },
            Mode::Command => match key {
                '\x1b' => self.mode = Mode::Normal,
                '\r' | '\n' => return self.execute_command(),
                '\x7f' | '\x08' => { self.command_buffer.pop(); }
                c if !c.is_control() => self.command_buffer.push(c),
                _ => {}
            },
        }
        true
    }

    fn execute_command(&mut self) -> bool {
        let cmd = self.command_buffer.as_str();
        let mut should_continue = true;
        match cmd {
            "w" => match self.save() {
                Ok(_) => self.status_msg = "Saved to output.txt".into(),
                Err(e) => self.status_msg = format!("Error: {}", e),
            },
            "q" => should_continue = false,
            "wq" => {
                let _ = self.save();
                should_continue = false;
            },
            _ => self.status_msg = format!("Unknown: {}", cmd),
        }
        self.mode = Mode::Normal;
        self.command_buffer.clear();
        should_continue
    }
    fn scroll(&mut self) {
        let visible_rows = (self.screen_rows - 1) as usize; // 상태바 제외

        // 커서가 현재 보이는 오프셋보다 위에 있으면 위로 스크롤
        if (self.cy as usize) < self.row_offset {
            self.row_offset = self.cy as usize;
        }
        // 커서가 현재 보이는 화면 끝보다 아래에 있으면 아래로 스크롤
        if (self.cy as usize) >= self.row_offset + visible_rows {
            self.row_offset = (self.cy as usize) - visible_rows + 1;
        }
    }
}

// --- Helper Functions ---
fn get_terminal_size() -> (u16, u16) {
    unsafe {
        let mut ws: winsize = std::mem::zeroed();
        if ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut ws) == -1 {
            return (80, 24);
        }
        (ws.ws_col, ws.ws_row)
    }
}

fn draw_screen(config: &EditorConfig) {
    let visible_rows = (config.screen_rows - 1) as usize;
    
    for y in 0..visible_rows {
        let file_row_idx = y + config.row_offset; // 오프셋 적용
        print!("\x1b[K"); // 현재 줄 지우기

        if file_row_idx < config.buffer.rows.len() {
            let mut line = config.buffer.rows[file_row_idx].content.clone();
            line.truncate(config.screen_cols as usize);
            print!("{}\r\n", line);
        } else {
            print!("~\r\n");
        }
    }
}

fn draw_status_bar(config: &EditorConfig) {
    print!("\x1b[{};1H\x1b[K", config.screen_rows);
    if config.mode == Mode::Command {
        print!(":{}", config.command_buffer);
    } else {
        let mode_str = match config.mode {
            Mode::Normal => "-- NORMAL --",
            Mode::Insert => "-- INSERT --",
            _ => "",
        };
        let status = format!("{} | Pos: {},{} | {}", mode_str, config.cx, config.cy, config.status_msg);
        print!("\x1b[7m{:width$}\x1b[m", status, width = config.screen_cols as usize);
    }
}
fn refresh_screen(config: &mut EditorConfig) { // 가변 참조로 변경
    config.scroll(); // 그리기 전 스크롤 계산

    print!("\x1b[?25l\x1b[H"); 
    draw_screen(config);
    draw_status_bar(config);

    // 커서 좌표 보정: (전체 줄 번호 - 오프셋)
    let screen_y = config.cy - config.row_offset as u16;
    print!("\x1b[{};{}H\x1b[?25h", screen_y + 1, config.cx + 1);
    
    io::stdout().flush().unwrap();
}
fn main() {
    let _raw_mode = RawMode::enable(); // 터미널을 로우 모드로 전환
    let mut config = EditorConfig::new(); // 에디터 설정 초기화

    // 1. 실행 인자 처리 (파일 열기)
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let filename = args[1].clone();
        // 파일 열기 시도
        if config.buffer.open(&filename).is_ok() {
            config.filename = Some(filename.clone());
            config.status_msg = format!("Opened: {}", filename);
        } else {
            // 파일이 없으면 새 파일로 간주
            config.filename = Some(filename.clone());
            config.status_msg = format!("New file: {}", filename);
        }
    }

    // 2. 초기 화면 청소
    print!("\x1b[2J");

    // 3. 메인 이벤트 루프
    loop {
        refresh_screen(&mut config); // 화면 갱신 (스크롤 및 커서 위치 계산 포함)

        let mut buf = [0; 1];
        // 표준 입력으로부터 한 바이트씩 읽음
        if io::stdin().read(&mut buf).is_ok() {
            let c = buf[0] as char;
            
            // 키 입력 처리 핸들러 호출
            // handle_keypress가 false를 반환하면 (:q 등) 루프 종료
            if !config.handle_keypress(c) {
                print!("\x1b[2J\x1b[H"); // 종료 전 화면 정리
                io::stdout().flush().unwrap();
                break;
            }
        }
    }
}
