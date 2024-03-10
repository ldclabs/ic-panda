#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

ic_cdk::export_candid!();

#[cfg(test)]
mod test {

    #[test]
    fn test_crowd_minting_formula() {
        println!("f = (ln(n) + 1)) / n");

        for n in [
            1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10, 20, 30, 40, 50, 100, 200, 500, 1000, 2000, 5000,
            10000, 50000, 100000, 1000000,
        ] {
            let t = (n as f32).ln() + 1f32;

            println!("f({}) -> {:.6}, sum: {:.2}", n, t / n as f32, t);
        }
    }
}
