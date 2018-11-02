extern crate libc;
use libc::{c_int, pid_t};
use std::ffi::{CString, CStr};
use std::io::{stdin, stdout, Write};
use std::ptr;

fn chdir(dir: &str) -> c_int {
    let dir = CString::new(dir).unwrap();
    unsafe {
        libc::chdir(dir.as_ptr())
    }
}

fn close(fd: c_int) -> c_int {
    unsafe {
        libc::close(fd)
    }
}

fn dup2(src: c_int, dst: c_int) -> c_int {
    unsafe {
        libc::dup2(src, dst)
    }
}

fn execvp(cmd: &Vec<String>) -> c_int {
    let prog: Vec<_> = cmd.iter().map(|s| CString::new(s.as_str()).unwrap()).collect();
    let mut prog: Vec<_> = prog.iter().map(|s| s.as_ptr()).collect();
    prog.push(ptr::null());
    unsafe {
        libc::execvp(prog[0], prog.as_ptr())
    }
}

fn exit(status: c_int) {
    unsafe {
        libc::exit(status)
    }
}

fn fork() -> pid_t {
    unsafe {
        libc::fork()
    }
}

fn getcwd() -> String {
    unsafe {
        let cwd = libc::getcwd(ptr::null_mut(), 0);
        CStr::from_ptr(cwd).to_str().unwrap().to_owned()
    }
}

fn kill(pid: pid_t) -> c_int {
    unsafe {
        libc::kill(pid, libc::SIGTERM)
    }
}

fn openr(path: &str) -> c_int {
    let path = CString::new(path).unwrap();
    unsafe {
        libc::open(path.as_ptr(), libc::O_RDONLY)
    }
}

fn openw(path: &str) -> c_int {
    let path = CString::new(path).unwrap();
    unsafe {
        libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_TRUNC | libc::O_CREAT, 0o644)
    }
}

fn perror(s: &str) {
    unsafe {
        let errno = *libc::__errno_location();
        let errnostr = libc::strerror(errno);
        let errnostr = CStr::from_ptr(errnostr).to_str().unwrap();
        eprintln!("{}: {} (errno {})", s, errnostr, errno);
    }
}

fn pipe(fds: &mut [c_int; 2]) -> c_int {
    unsafe {
        libc::pipe(fds.as_mut_ptr())
    }
}

fn waitpid(pid: pid_t, options: c_int) -> pid_t {
    unsafe {
        libc::waitpid(pid, ptr::null_mut(), options)
    }
}

struct Cmd {
    cmd: Vec<String>,
}

impl Cmd {
    fn new() -> Self {
        Self {
            cmd: Vec::new(),
        }
    }

    fn push(&mut self, s: &str) {
        self.cmd.push(s.to_owned());
    }

    fn prog(&self) -> &str {
        &self.cmd[0]
    }

    fn is_builtin(&self) -> bool {
        match self.prog() {
            "cd" | "exit" | "history" | "jobs" | "kill" | "pwd" => true,
            _ => false,
        }
    }

    fn prog_num(&self, num: usize) -> bool {
        if self.cmd.len()-1 != num {
            eprintln!("{}: Expect {} arguments, found {}", self.prog(), num, self.cmd.len()-1);
            false
        } else {
            true
        }
    }

    fn exec(&self, history: &Vec<String>, jobs: &Vec<(Vec<pid_t>, String)>) {
        match self.prog() {
            "cd" => {
                if self.prog_num(1) {
                    let dir = &self.cmd[1];
                    let ret = chdir(dir);
                    if ret == -1 {
                        perror(&("cd: ".to_owned() + dir));
                    }
                }
            },
            "history" => {
                if self.prog_num(0) {
                    let mut hisno = 0;
                    for cmd in history {
                        hisno += 1;
                        println!("{:>5}  {}", hisno, cmd);
                    }
                }
            },
            "jobs" => {
                if self.prog_num(0) {
                    for cmd in jobs {
                        for pid in &cmd.0 {
                            if waitpid(*pid, libc::WNOHANG) == 0 {
                                println!("{}", cmd.1);
                                break;
                            }
                        }
                    }
                }
            },
            "exit" => {
                if self.prog_num(0) {
                    exit(0);
                }
            },
            "kill" => {
                if self.prog_num(1) {
                    let arg = &self.cmd[1];
                    match arg.parse::<pid_t>() {
                        Ok(pid) => {
                            let ret = kill(pid);
                            if ret == -1 {
                                perror("kill");
                            }
                        },
                        Err(_) => {
                            eprintln!("kill: {} isn't an integer", arg);
                        },
                    }
                }
            },
            "pwd" => {
                if self.prog_num(0) {
                    println!("{}", getcwd());
                }
            },
            _ => {
                let ret = execvp(&self.cmd);
                if ret == -1 {
                    perror(self.prog());
                }
            },
        }
    }
}

struct CmdLine {
    cmds: Vec<Cmd>,
    back: bool,
    filein: Option<String>,
    fileout: Option<String>,
}

impl CmdLine {
    fn new(line: &str) -> Option<Self> {
        let tokens: Vec<_> = line.split_whitespace().collect();
        let mut top = true;
        let mut cmds = Vec::new();
        let mut back = false;
        let mut filein = None;
        let mut fileout = None;
        let mut cmdno = 0;
        for i in 0 .. tokens.len() {
            match tokens[i] {
                "&" => {
                    if i != tokens.len()-1 {
                        eprintln!("Parsing Error: & can appear only after the last command");
                        return None;
                    }
                    back = true;
                },
                "|" => {
                    if i == 0 || tokens[i-1] == "|" {
                        eprintln!("Parsing Error: | cannot appear as the first word in a command");
                        return None;
                    }
                    cmdno += 1;
                    top = true;
                }
                "<" => {
                    if i == tokens.len()-1 {
                        eprintln!("Parsing Error: No filename after <");
                        return None;
                    }
                    if let Some(_) = "&|<>".find(tokens[i+1]) {
                        eprintln!("Parsing Error: Illegal filename after <");
                        return None;
                    }
                    if cmdno > 0 {
                        eprintln!("Parsing Error: < can appear only in the first command");
                        return None;
                    }
                    filein = Some(tokens[i+1].to_owned());
                }
                ">" => {
                    if i == tokens.len()-1 {
                        eprintln!("Parsing Error: No filename after >");
                        return None;
                    } else if let Some(_) = "&|<>".find(tokens[i+1]) {
                        eprintln!("Parsing Error: Illegal filename after >");
                        return None;
                    }
                    for j in i+1 .. tokens.len() {
                        if tokens[j] == "|" {
                            eprintln!("Parsing Error: > can appear only in the last command");
                            return None;
                        }
                    }
                    fileout = Some(tokens[i+1].to_owned());
                }
                _ => {
                    if i == 0 || (tokens[i-1] != "<" && tokens[i-1] != ">") {
                        if top {
                            cmds.push(Cmd::new());
                            top = false;
                        }
                        cmds[cmdno].push(&tokens[i]);
                    }
                },
            }
        }
        Some(Self {
            cmds,
            filein,
            fileout,
            back,
        })
    }

    fn len(&self) -> usize {
        self.cmds.len()
    }

    fn dupin(&self) {
        if let Some(ref path) = self.filein {
            let fdin = openr(path);
            if fdin == -1 {
                perror(&("I/O Error: ".to_owned() + path));
                exit(1);
            } else {
                dup2(fdin, 0);
                close(fdin);
            }
        }
    }

    fn dupout(&self) {
        if let Some(ref path) = self.fileout {
            let fdout = openw(path);
            if fdout == -1 {
                perror(&("I/O Error: ".to_owned() + path));
                exit(1);
            } else {
                dup2(fdout, 1);
                close(fdout);
            }
        }
    }

    fn exec(&self, history: &Vec<String>, jobs: &Vec<(Vec<pid_t>, String)>) -> Vec<pid_t> {
        let mut pids = Vec::new();
        if self.len() == 1 {
            if self.cmds[0].is_builtin() {
                self.cmds[0].exec(history, jobs);
            } else {
                let pid = fork();
                pids.push(pid);
                if pid == 0 {
                    self.dupin();
                    self.dupout();
                    self.cmds[0].exec(history, jobs);
                    exit(0);
                }
            }
        } else if self.len() > 0 {
            let len = self.len();
            let mut fd = vec![[0; 2]; len-1];
            for i in 0 .. len-1 {
                pipe(&mut fd[i]);
            }
            let pid = fork();
            pids.push(pid);
            if pid == 0 {
                self.dupin();
                dup2(fd[0][1], 1);
                self.cmds[0].exec(history, jobs);
                exit(0);
            }
            close(fd[0][1]);
            for i in 1 .. len-1 {
                let pid = fork();
                pids.push(pid);
                if pid == 0 {
                    dup2(fd[i-1][0], 0);
                    dup2(fd[i][1], 1);
                    self.cmds[i].exec(history, jobs);
                    exit(0);
                }
                close(fd[i-1][0]);
                close(fd[i][1]);
            }
            let pid = fork();
            pids.push(pid);
            if pid == 0 {
                self.dupout();
                dup2(fd[len-2][0], 0);
                self.cmds[len-1].exec(history, jobs);
                exit(0);
            }
            close(fd[len-2][0]);
        }
        pids
    }
}

struct Rush {
    history: Vec<String>,
    jobs: Vec<(Vec<pid_t>, String)>,
}

impl Rush {
    fn new() -> Self {
        Self {
            history: Vec::new(),
            jobs: Vec::new(),
        }
    }

    fn run(&mut self) {
        loop {
            print!("$ ");
            if let Err(error) = stdout().flush() {
                eprintln!("I/O Error: {}", error);
                exit(1);
            }
            let mut line = String::new();
            if let Err(error) = stdin().read_line(&mut line) {
                eprintln!("I/O Error: {}", error);
                exit(1);
            }
            if line.len() == 0 {
                exit(0);
            }
            if line.find('\0').is_some() {
                eprintln!("nul byte found in the input");
                continue;
            }
            if line.as_bytes()[line.len()-1] as char == '\n' {
                line.pop();
            }
            let cmdline = CmdLine::new(&line);
            if let Some(cmdline) = cmdline {
                let pids = cmdline.exec(&self.history, &self.jobs);
                if cmdline.back {
                    let cmd = line.replace("&", "").split_whitespace().collect::<Vec<_>>().join(" ");
                    self.jobs.push((pids, cmd));
                } else {
                    for pid in pids {
                        waitpid(pid, 0);
                    }
                }
            }
            self.history.push(line);
        }
    }
}

fn main() {
    let mut rush = Rush::new();
    rush.run();
}
