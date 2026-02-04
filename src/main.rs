use libc::{tcgetattr, tcsetattr, termios, ECHO, ICANON, TCSAFLUSH, STDIN_FILENO,ioctl,winsize,TIOCGWINSZ};
use std::io::{self, Read};
use std::mem;
use libc::STDOUT_FILENO;
struct RawMode {
    orig_termios: termios,
}

impl RawMode {
    fn enable() -> Self {
        unsafe {
            let mut raw: termios = mem::zeroed();
            // 현재 터미널 설정을 가져옴
            if tcgetattr(STDIN_FILENO, &mut raw) == -1 {
                panic!("tcgetattr 실패");
            }

            let orig_termios = raw; // 복구를 위해 원본 저장

            // 로우 모드 설정: ECHO와 ICANON 플래그를 비트 연산으로 제거
            raw.c_lflag &= !(ECHO | ICANON);

            // 설정을 즉시 적용 (TCSAFLUSH: 출력 버퍼를 비운 뒤 적용)
            if tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw) == -1 {
                panic!("tcsetattr 실패");
            }

            RawMode { orig_termios }
        }
    }
}
struct EditorConfig {
    cx: u16,      // 커서 X 좌표 (컬럼)
    cy: u16,      // 커서 Y 좌표 (로우)
    screen_cols: u16,
    screen_rows: u16,
}

impl EditorConfig {
    fn new() -> Self {
        let (cols, rows) = get_terminal_size();
        EditorConfig {
            cx: 0,
            cy: 0,
            screen_cols: cols,
            screen_rows: rows,
        }
    }

    // 커서 이동 로직 (경계 검사 포함)
    fn move_cursor(&mut self, key: char) {
        match key {
            'h' => if self.cx > 0 { self.cx -= 1 },
            'j' => if self.cy < self.screen_rows - 1 { self.cy += 1 },
            'k' => if self.cy > 0 { self.cy -= 1 },
            'l' => if self.cx < self.screen_cols - 1 { self.cx += 1 },
            _ => {}
        }
    }
}
fn get_terminal_size() -> (u16, u16) {
    unsafe {
        let mut ws: winsize = std::mem::zeroed();
        // ioctl을 통해 터미널 윈도우 사이즈 정보를 요청
        if ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut ws) == -1 {
            // 실패 시 기본값 반환 (보통 80x24)
            return (80, 24);
        }
        (ws.ws_col, ws.ws_row)
    }
}
fn draw_screen(cols: u16, rows: u16) {
    // 1. 화면 전체를 지우고 커서를 맨 위로 보냄
    // \x1b[2J : 전체 화면 지우기
    // \x1b[H  : 커서를 1,1 위치로 이동
    print!("\x1b[2J\x1b[H");

    for y in 0..rows {
        if y == rows / 3 {
            // 화면 1/3 지점에 간단한 환영 메시지 출력
            let welcome = "Vii editor -- version 0.1.0";
            let padding = (cols as usize - welcome.len()) / 2;
            print!("~{:>width$}\r\n", welcome, width = padding + welcome.len());
        } else if y < rows - 1 {
            // 일반적인 빈 줄 표시
            print!("~\r\n");
        } else {
            // 마지막 줄은 줄바꿈 없이 출력 (스크롤 방지)
            print!("~");
        }
    }
    // 다시 커서를 맨 위로
    print!("\x1b[H");
    use std::io::{self, Write};
    io::stdout().flush().unwrap(); // 버퍼를 비워 즉시 화면에 뿌림
}
// 프로그램이 끝날 때(또는 패닉 시) 자동으로 터미널 복구
impl Drop for RawMode {
    fn drop(&mut self) {
        unsafe {
            tcsetattr(STDIN_FILENO, TCSAFLUSH, &self.orig_termios);
        }
    }
}
fn refresh_screen(config: &EditorConfig) {
    // 1. 커서를 일단 숨김 (깜빡임 방지)
    print!("\x1b[?25l");
    // 2. 커서를 맨 위로 보내고 다시 배경 그리기 (여기서는 단순화를 위해 전체 다시 그림)
    print!("\x1b[H");
    draw_screen(config.screen_cols, config.screen_rows);
    
    // 3. 계산된 좌표로 커서 이동
    // ANSI 좌표는 1부터 시작하므로 1을 더해줍니다.
    print!("\x1b[{};{}H", config.cy + 1, config.cx + 1);
    
    // 4. 커서를 다시 보임
    print!("\x1b[?25h");
    
    use std::io::{self, Write};
    io::stdout().flush().unwrap();
}
fn main() {
    let _raw_mode = RawMode::enable();
    let mut config = EditorConfig::new();

    loop {
        refresh_screen(&config);

        // 한 바이트씩 입력 대기
        if let Some(byte) = std::io::stdin().bytes().next() {
            let c = byte.unwrap() as char;

            if c == 'q' {
                // 종료 시 화면을 깨끗이 지우고 종료
                print!("\x1b[2J\x1b[H");
                break;
            }

            // h, j, k, l 입력 처리
            config.move_cursor(c);
        }
    }
}
