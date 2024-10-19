use libc::{self, dup, dup2, exit, EXIT_SUCCESS, STDIN_FILENO, STDOUT_FILENO};
// use std::default;
use std::env;
use std::ffi::CStr;
use std::ffi::CString;
use std::io;
use std::io::Write;
use std::os::raw::c_char;
// use std::process;
use std::ptr;

struct OrigFiledes {
    input: i32,
    output: i32,
}

impl OrigFiledes {
    fn default() -> OrigFiledes {
        OrigFiledes {
            input: 0,
            output: 0,
        }
    }
}
fn main() {
    loop {
        let cwd = env::current_dir().expect("Failed to get current directory"); // Получаем текущую директорию
        print!("{}> ", cwd.display());
        std::io::stdout().flush().unwrap(); //
        let mut input = String::new();

        // Сохраняем оригинальные файловые дискрипторы, для того чтобы не потерять их при замене
        let mut orig_filedes = OrigFiledes::default();
        unsafe {
            orig_filedes.input = dup(STDIN_FILENO);
            orig_filedes.output = dup(STDOUT_FILENO);
        }

        io::stdin().read_line(&mut input).unwrap(); // Считываем строку

        // Если строка содержит разделитель "|" - обрабатываем конвейер
        if input.contains("|") {
            let mut filedes: [i32; 2] = [0, 0]; // Массив дискрипторов канала
            let input_conveyor: Vec<String> = input.split("|").map(|s| s.to_string()).collect(); // Разбиваем строку по разделителю "|"
                                                                                                 // Проходим по каждой подстроке
            for (i, input) in input_conveyor
                .iter()
                .enumerate()
                .map(|(i, s)| (i, s.clone()))
            {
                unsafe {
                    // Первая команда конвейера
                    if i == 0 {
                        libc::pipe(filedes.as_mut_ptr()); // Создаем канал
                        if dup2(filedes[1], STDOUT_FILENO) < 0 {
                            // Дублируем записывающий конец канала в STDOUT первой команды
                            eprintln!("Error dup");
                        }
                        libc::close(filedes[1]); // Закрываем конец канала

                    // Последняя команда конвейера
                    } else if i == input_conveyor.len() - 1 {
                        // Дублируем читаемый конец канала в STDIN последней команды
                        if dup2(filedes[0], STDIN_FILENO) < 0 {
                            eprintln!("Error dup");
                        }
                        // Дублируем оригинальный вывод в STDOUT последней команды
                        dup2(orig_filedes.output, STDOUT_FILENO);
                    }
                    // Промежуточные команды
                    else {
                        // Дублируем читаемый конец канала предыдущей команды в STDIN
                        if dup2(filedes[0], STDIN_FILENO) < 0 {
                            eprintln!("Error dup");
                        }
                        // Создаем новый канал*/
                        libc::pipe(filedes.as_mut_ptr());

                        //Дублируем записывающий конец нового канала в STDOUT
                        if dup2(filedes[1], libc::STDOUT_FILENO) < 0 {
                            eprintln!("Error dup");
                        }
                        // Закрываем конец канала
                        libc::close(filedes[1]);
                    }
                    // Вызываем функцию обработки команды
                    comand_handler(input, Some(&orig_filedes));
                }
            }
        } else {
            unsafe {
                // Вызываем функцию обработки команды без указания дискрипторов файлов, т.к. перенаправлять вывод не нужно
                comand_handler(input, None);
            }
        }
    }
}

// Функция для обработки команды
unsafe fn comand_handler(input: String, orig_filedes: Option<&OrigFiledes>) {
    // Разбиваем команду по пробелам
    let input_vec: Vec<&str> = input.split_whitespace().collect();
    let mut input_c_strings: Vec<CString> = Vec::new(); // Массив строк в виде последовательности байтов

    // Добавляем каждую подстроку в вектор C-строк
    for word in &input_vec {
        input_c_strings.push(CString::new(*word).expect("Error"));
    }

    // Получаем вектор указателей на подстроки
    let mut input_ptrs: Vec<*const c_char> = input_c_strings.iter().map(|s| s.as_ptr()).collect();

    let null: *const i8 = ptr::null();
    input_ptrs.push(null); // Добавляем NULL в конец вектора

    // Осуществляем системный вызов для клонирования текущего процесса
    let pid = libc::fork();
    if pid < 0 {
        eprintln!("Error");
    }
    // Родительский процесс
    else if pid > 0 {
        let c_str = CStr::from_ptr(input_ptrs[0]);
        if c_str.to_string_lossy() == "cd" {
            // Если введена команда "cd" меняем текущую директорию родительского процесса
            if input_ptrs[1] != ptr::null() {
                if libc::chdir(input_ptrs[1]) != 0 {
                    // Осуществляем системный вызов для смены директории
                    eprintln!("no such file or directory");
                }
            }
            // Если не указано аргументов перемещаемся в домашнюю директорию
            else {
                let home = env::var("HOME").unwrap();
                let home = CString::new(home).unwrap();
                if libc::chdir(home.as_ptr()) != 0 {
                    eprintln!("no such file or directory");
                }
            }
        }
        // Если введена команда "exit" завершаем работу программы
        else if c_str.to_string_lossy() == "exit" {
            std::process::exit(EXIT_SUCCESS);
        }
        // В остальных случаях ожидаем завершения дочернего процесса
        else {
            let mut status = 0;
            // Если текущая команда является частью конвейера - дублируем оригинальные дескрипторы ввода и вывода в STDIN и STDOUT
            if let Some(orig_filedes) = orig_filedes {
                dup2(orig_filedes.input, STDIN_FILENO);
                dup2(orig_filedes.output, STDOUT_FILENO);
            }
            loop {
                libc::waitpid(pid, &mut status, libc::WUNTRACED);
                if libc::WIFEXITED(status) || libc::WIFSIGNALED(status) {
                    break;
                }
            }
        }
    }
    // Дочерний процесс
    else {
        let c_str = CStr::from_ptr(input_ptrs[0]);
        // Если введена команда "cd" или "exit" - завершаем процесс
        if c_str.to_string_lossy() == "cd" || c_str.to_string_lossy() == "exit" {
            exit(EXIT_SUCCESS);
        }
        // Иначе выполняем системный вызов для запуска исполняемого файла и замены текущего процесса
        else {
            if libc::execvp(input_ptrs[0], input_ptrs.as_ptr()) == -1 {
                eprintln!("Error");
            }
        }
        exit(EXIT_SUCCESS);
    }
}
