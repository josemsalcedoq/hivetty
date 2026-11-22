//! Binary-size probe: only libghostty-vt (feed stdin, print the screen).

use std::io::Read;

use libghostty_vt::Terminal;
use libghostty_vt::render::{CellIterator, RenderState, RowIterator};

fn main() {
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    let mut term = Terminal::new(80, 24).expect("terminal");
    term.vt_write(&input);
    let mut state = RenderState::new().expect("render state");
    let snap = state.update(&term).expect("update");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut it = rows.update(&snap).expect("rows");
    let mut text = String::new();
    while let Some(row) = it.next() {
        let mut c = cells.update(row).expect("cells");
        while let Some(cell) = c.next() {
            text.extend(cell.graphemes().expect("graphemes").first());
        }
    }
    println!("{text}");
}
