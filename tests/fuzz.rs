//! Random programs, checked the same way as the golden cases: the compiler
//! must read the formatted file as the same program, formatting twice must
//! change nothing, and lines must fit wherever they can be broken.

mod common;

/// A small deterministic generator, so failures can be replayed by seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

const NAMES: &[&str] = &[
    "x",
    "gain",
    "phase",
    "a_rather_long_identifier",
    "another_quite_long_name_here",
    "frequency_modulation_depth",
    "E4",
    "F#4",
];
const NUMBERS: &[&str] = &[
    "0", "1", "0.5", "440Hz", "300ms", "-6dB", "48_000", "1e-3", "7st",
];
const OPS: &[&str] = &["+", "-", "*", "/", "%", "&&", "||", "<", ">=", "==", "!="];
const CALLEES: &[&str] = &["sine", "f", "some_long_function_name", "equal"];

struct Gen {
    rng: Rng,
}

impl Gen {
    /// Whitespace between two tokens: usually a space, sometimes a comment.
    /// `newline_ok` says whether a line break is allowed here.
    fn gap(&mut self, newline_ok: bool) -> String {
        match self.rng.below(20) {
            0 => " /* note */ ".into(),
            3 if newline_ok => "\n/* a block\n   over lines */\n".into(),
            1 if newline_ok => " // remark\n".into(),
            2 if newline_ok => "\n".into(),
            _ => " ".into(),
        }
    }

    fn expr(&mut self, depth: u32, nest: bool) -> String {
        let atom = depth == 0 || self.rng.chance(25);
        if atom {
            return if self.rng.chance(50) {
                self.rng.pick(NAMES).into()
            } else {
                self.rng.pick(NUMBERS).into()
            };
        }
        let d = depth - 1;
        match self.rng.below(12) {
            0 | 1 => {
                let op = self.rng.pick(OPS);
                // Comparisons cannot be chained, so wrap a comparison operand.
                let lhs = self.expr(d, nest);
                let rhs = self.expr(d, nest);
                let gap = self.gap(true);
                let before = if nest && self.rng.chance(20) {
                    "\n"
                } else {
                    " "
                };
                format!("({lhs}){before}{op}{gap}({rhs})")
            }
            2 => {
                let op = self.rng.pick(&["-", "!", "+"]);
                format!("{op}{}", self.expr(d, nest))
            }
            3 | 4 => {
                let callee = self.rng.pick(CALLEES);
                let n = self.rng.below(4);
                let args: Vec<String> = (0..n)
                    .map(|i| {
                        let mut value = self.expr(d, true);
                        if self.rng.chance(15) {
                            value = format!("each {value}");
                        }
                        let gap = self.gap(true);
                        if i > 0 && self.rng.chance(30) {
                            format!("{gap}name{i}: {value}")
                        } else {
                            format!("{gap}{value}")
                        }
                    })
                    .collect();
                let trailing = if n > 0 && self.rng.chance(30) {
                    ","
                } else {
                    ""
                };
                format!("{callee}({}{trailing})", args.join(","))
            }
            5 => {
                let input = self.expr(d, nest);
                let callee = self.rng.pick(CALLEES);
                let before = if self.rng.chance(30) { "\n" } else { " " };
                let args = if self.rng.chance(50) {
                    format!("({})", self.expr(d, true))
                } else {
                    String::new()
                };
                format!("({input}){before}|> {callee}{args}")
            }
            6 => {
                let n = 1 + self.rng.below(4);
                let items: Vec<String> = (0..n).map(|_| self.expr(d, true)).collect();
                format!("[{}]", items.join(&format!(",{}", self.gap(true))))
            }
            7 => format!("({})[{}]", self.expr(d, nest), self.expr(d, true)),
            8 => {
                let cond = self.expr(d, false);
                let then = self.block(d, true);
                let els = match self.rng.below(3) {
                    0 => String::new(),
                    1 => format!(" else {}", self.block(d, true)),
                    _ => format!(" else if {} {}", self.expr(d, false), self.block(d, true)),
                };
                format!("if ({cond}) {then}{els}")
            }
            9 => {
                let typed = self.rng.chance(50);
                let params = if typed { "p: Pitch, q: Float" } else { "p" };
                let ret = if typed { " Freq" } else { "" };
                format!("fn({params}){ret} {}", self.block(d, true))
            }
            10 => {
                let ty = self
                    .rng
                    .pick(&["Int", "Float", "[Sample; 2]", "fn(Pitch) Freq"]);
                format!("({}) as {ty}", self.expr(d, nest))
            }
            _ => format!("({}).field", self.expr(d, nest)),
        }
    }

    fn block(&mut self, depth: u32, tail_expr: bool) -> String {
        let n = self.rng.below(3);
        let mut out = String::from("{");
        for _ in 0..n {
            out.push('\n');
            out.push_str(&self.stmt(depth));
        }
        if tail_expr || n == 0 {
            out.push('\n');
            out.push_str(&self.expr(depth, false));
        }
        out.push_str("\n}");
        out
    }

    fn stmt(&mut self, depth: u32) -> String {
        let comment = if self.rng.chance(15) {
            "// about this\n"
        } else {
            ""
        };
        let blank = if self.rng.chance(15) { "\n" } else { "" };
        let body = match self.rng.below(7) {
            0 => format!("let v = {}", self.expr(depth, false)),
            5 => format!("const K: Float = {}", self.expr(depth, false)),
            1 => format!("state s: Sample = {}", self.expr(depth, false)),
            2 => format!("x = {}", self.expr(depth, false)),
            3 => format!("return {}", self.expr(depth, false)),
            4 => format!(
                "on note_on(note) {}",
                self.block(depth.saturating_sub(1), false)
            ),
            _ => self.expr(depth, false),
        };
        let semi = if self.rng.chance(10) { ";" } else { "" };
        format!("{blank}{comment}{body}{semi}")
    }

    fn program(&mut self) -> String {
        let mut out = String::new();
        for i in 0..self.rng.below(3) {
            let export = if self.rng.chance(30) { "export " } else { "" };
            let ext = if self.rng.chance(30) { ".rill" } else { "" };
            out.push_str(&format!("{export}import \"lib/m{i}{ext}\"\n"));
        }
        for i in 0..self.rng.below(3) {
            let semi = if self.rng.chance(20) { ";" } else { "" };
            let export = if self.rng.chance(30) { "export " } else { "" };
            out.push_str(&format!(
                "{export}const TOP_{i} = {}{semi}\n",
                self.expr(1, false)
            ));
        }
        for i in 0..1 + self.rng.below(3) {
            let params: Vec<String> = (0..self.rng.below(4))
                .map(|j| format!("param_number_{j}: Sample = {}", self.expr(1, true)))
                .collect();
            let export = if self.rng.chance(30) { "export " } else { "" };
            out.push_str(&format!(
                "{export}rill item{i}({}) [Sample; 2] {{\n",
                params.join(", ")
            ));
            for _ in 0..1 + self.rng.below(4) {
                out.push_str(&self.stmt(3));
                out.push('\n');
            }
            out.push_str("}\n");
        }
        out
    }
}

#[test]
fn random_programs() {
    // `FUZZ_SEEDS=100000 cargo test --test fuzz` for a longer run.
    let seeds: u64 = std::env::var("FUZZ_SEEDS").map_or(3000, |s| s.parse().unwrap());
    let mut checked = 0;
    for seed in 1..=seeds {
        let mut g = Gen {
            rng: Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1),
        };
        let src = g.program();
        // The generator is loose; only programs the compiler accepts count.
        if rill::lang::parse(&src).is_err() {
            continue;
        }
        common::check_properties(&format!("seed {seed}:\n{src}"), &src);
        checked += 1;
    }
    assert!(checked > seeds / 3, "only {checked} programs parsed");
}
