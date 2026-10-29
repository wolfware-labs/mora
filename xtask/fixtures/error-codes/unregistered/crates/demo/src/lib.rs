fn report() -> Code {
    codes::MR0001
}

fn later() -> Code {
    let _ = codes::MR00012;
    codes::MR0001.clone()
}
