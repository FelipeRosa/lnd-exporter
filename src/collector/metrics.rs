pub fn num_peers_total() -> prometheus::IntGauge {
    prometheus::IntGauge::new(
        "lnd_num_peers_total",
        "Number of peers connected to the lnd node",
    )
    .expect("valid metric")
}

pub fn block_height() -> prometheus::IntGauge {
    prometheus::IntGauge::new("lnd_block_height", "Chain block height").expect("valid metric")
}

pub fn outgoing_payments() -> prometheus::IntGaugeVec {
    prometheus::IntGaugeVec::new(
        prometheus::Opts::new(
            "lnd_outgoing_payments",
            "Number of outgoing payments on the lnd node",
        ),
        &["status"],
    )
    .expect("valid metric")
}

pub fn payment_failure_reasons() -> prometheus::IntGaugeVec {
    prometheus::IntGaugeVec::new(
        prometheus::Opts::new("lnd_payment_failure_reasons", "Payment failure reasons"),
        &["reason"],
    )
    .expect("valid metric")
}

pub fn channel_balance_total_sat() -> prometheus::IntGaugeVec {
    prometheus::IntGaugeVec::new(
        prometheus::Opts::new(
            "lnd_channel_balance_total_sat",
            "Individual channel balances",
        ),
        &["chan_id", "active", "channel_point", "category"],
    )
    .expect("valid metric")
}

pub fn total_fee_msat() -> prometheus::IntGauge {
    prometheus::IntGauge::new("lnd_total_fee_msat", "Total fee paid").expect("valid metric")
}
