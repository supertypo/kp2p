use vergen_git2::{Emitter, Git2Builder};

fn main() {
    let git2 = Git2Builder::default().branch(true).commit_date(true).sha(true).describe(true, true, None).build().unwrap();
    Emitter::default().add_instructions(&git2).unwrap().emit().unwrap();
}
