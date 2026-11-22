//! Binary-size probe: only alacritty_terminal (feed stdin, print the screen).

use std::io::Read;

use alacritty_terminal::Term;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

struct Dims;
impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        24
    }
    fn screen_lines(&self) -> usize {
        24
    }
    fn columns(&self) -> usize {
        80
    }
}

fn main() {
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    let mut term = Term::new(Config::default(), &Dims, VoidListener);
    let mut p: Processor<StdSyncHandler> = Processor::new();
    p.advance(&mut term, &input);
    let text: String = term
        .renderable_content()
        .display_iter
        .map(|c| c.c)
        .collect();
    println!("{text}");
}
