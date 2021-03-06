extern crate libc;

use self::libc::{c_int, c_char};
use ncurses::window::Window;


/// The character to use for a bar
const BAR_CHAR: c_char = '|' as c_char;


/// The character to use for rows above the bar
const EMPTY_CHAR: c_char = ' ' as c_char;


/// The character to use where there is a lack of data due to scaling issues.
/// (If the user sees this character, it is because the visualizer wasn't
/// properly scaled to the window width)
const BORDER_CHAR: c_char = ' ' as c_char;


/// The character to initialize the row arrays with.
/// (This is not the same as EMPTY_CHAR so that it is easy to detect that we
/// didn't draw some part of the screen. Users should never see this.)
const INIT_CHAR: c_char = '#' as c_char;


/// Scales down a vector by averaging the elements between the resulting points
pub fn scale_fft_output(input: &[f64], new_len: usize) -> Vec<f64> {
    if new_len >= input.len() {
        return input.to_vec();
    }

    let band_size: usize = input.len() / new_len;
    assert!(band_size > 0);
    let mut output: Vec<f64> = Vec::with_capacity(new_len);

    let mut temp_count: usize = 0;
    let mut sum: f64 = 0.0;

    for &x in input.iter() {
        if temp_count >= band_size {
            let avg: f64 = sum/temp_count as f64;
            output.push(avg);
            temp_count = 0;
            sum = 0.0;
        } else {
            sum += x;
            temp_count+=1;
        }
    }

    if temp_count >= band_size {
        output.push(sum/temp_count as f64);
    }

    output
}



/// Loops through an iterator of f64 and gets the min and max values.
/// The min/max functions in the standard library don't work on floats.
fn get_min_max<'a, I: Iterator<Item=&'a f64>>(iter: &'a mut I) -> (f64, f64) {
    let mut min: f64 = 0.0;
    let mut max: f64 = 0.0;
    for &x in iter {
        if x < min {
            min = x;
        }
        if x > max {
            max = x;
        }
    }
    (min, max)
}

/// Resize the row buffer to width
fn resize_rowbuf(row: &mut Vec<c_char>, width: usize) {
    while row.len() < width {
        row.push(INIT_CHAR);
    }
    while row.len() > width {
        row.pop().unwrap();
    }
    row.shrink_to_fit();
}

pub struct Visualizer{
   // The ncurses Window object
   win: Window,
   // A buffer of characters for a row on the screen (used to reduce calls to
   // the ncurses addstr function)
   rows: Vec<Vec<c_char>>,
   // The width of the window the last time the animation was called
   width: usize,
   // The height of the window the last time the animation was called
   height: usize
}


impl Visualizer {
    /// Instantiate a new visualizer. Takes over the terminal with ncurses.
    pub fn new() -> Visualizer {
        let mut win = Window::new();

        // Disable the cursor so it's not moving all around the screen when the
        // animation is rendering.
        match win.curs_set(0) {
            Err(_) => panic!("Failed to disable cursor!"),
            Ok(_) => {}
        }

        Visualizer{
            win: win,
            rows: Vec::new(),
            width: 0,
            height: 0
        }
    }

    /// Get the width of the scren in columns. Callers can use this to
    /// determine the minimum amount of data the animation needs to fill the
    /// screen.
    pub fn get_width(&self) -> usize {
        self.win.get_max_x().unwrap() as usize - 1
    }

    /// Adds or removes rows if the window size is changed.
    fn update_row_count(&mut self, height: usize) {
        while self.rows.len() < height {
            self.rows.push(Vec::new());
        }
        while self.rows.len() > height {
            self.rows.pop();
        }
    }

    /// Resizes each of hte row buffers to the given width
    fn resize_rowbufs(&mut self, width: usize) {
        for row in self.rows.iter_mut() {
            resize_rowbuf(row, width);
        }
    }

    /// Do any necessary adjustments for a window size change. This gets
    /// called when we fetch the max_yx
    fn update_size(&mut self) {
        let (max_y, max_x) = self.win.get_max_yx().unwrap();
        let height: usize = max_y as usize;
        let width: usize = max_x as usize - 1;

        if self.width != width || self.height != height {
            self.update_row_count(height);
            self.resize_rowbufs(width);
            self.width = width;
            self.height = height;
        }
    }

    /// Render a single frame of the animation
    pub fn render_frame(&mut self, data: &[f64]) -> Result<(), c_int> {
        self.update_size();

        let data = scale_fft_output(data, self.width as usize);
        let (_, max_val) = get_min_max(&mut data.iter());
        let scaled: Vec<usize> = data.iter()
            .map(|&x| {
                if x < 1.0 {
                    0
                } else {
                    ((x / max_val) * (self.height as f64 - 1.0)) as usize
                }
            })
            .collect();

        for (y, row) in self.rows.iter_mut().enumerate().rev() {
            for (x, val) in row.iter_mut().enumerate() {
                *val = if x >= scaled.len() {
                    BORDER_CHAR
                } else {
                    let val = scaled[x];
                    if val >= y {
                        BAR_CHAR
                    } else {
                        EMPTY_CHAR
                    }
                };
            }

            match self.win.addbytes((self.height - y -1) as c_int, 0, row) {
                Err(_) => {
                    // Happens when window is resized. Skip the frame.
                    return Ok(());
                },
                Ok(_) => { }
            }
        }

        // Add some info so you can see the decisions it's making
        let debuginfo = format!(" width: {}, height: {}, bars: {} ", self.width, self.height, scaled.len());
        let _ = self.win.addstr(0, (self.width - debuginfo.len()) as c_int, &debuginfo[..]);

        // Calling refresh makes it actually take effect
        try!(self.win.refresh());

        Ok(())
    }
}


unsafe impl Send for Visualizer {}
