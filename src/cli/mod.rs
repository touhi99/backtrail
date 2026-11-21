mod dispatch;
mod kg;
mod setup;

struct Args(Vec<String>);

impl Args {
    fn opt(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .position(|a| a == name)
            .and_then(|i| self.0.get(i + 1))
            .cloned()
    }
    fn text(&self, name: &str) -> String {
        self.opt(name).unwrap_or_default()
    }
    fn num<T: std::str::FromStr>(&self, name: &str, default: T) -> T {
        self.opt(name)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
    fn has(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }
    fn at(&self, n: usize, what: &str) -> String {
        self.0
            .get(n)
            .cloned()
            .unwrap_or_else(|| fail(format!("missing {what}")))
    }
    fn rest(&self, n: usize) -> String {
        self.0.get(n..).unwrap_or_default().join(" ")
    }
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("rejected: {message}");
    std::process::exit(1)
}

pub(crate) fn run() {
    dispatch::run();
}
