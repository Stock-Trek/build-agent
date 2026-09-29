use std::cmp::Ordering;
use stock_trek::prelude::*;

pub struct CostAveraging {
    key_cex: SignalKey<CexId>,
    key_account: SignalKey<AccountId>,
    key_market_exists: SignalKey<bool>,
    key_cheapest_price: SignalKey<f64>,
    key_satoshi_quantity: SignalKey<f64>,
}

impl Default for CostAveraging {
    fn default() -> Self {
        Self {
            key_cex: SignalKey::new_required("CEX"),
            key_account: SignalKey::new_required("ACCOUNT"),
            key_market_exists: SignalKey::new_optional("MARKET_EXISTS", false),
            key_cheapest_price: SignalKey::new_required("CHEAPEST_PRICE"),
            key_satoshi_quantity: SignalKey::new_required("SATOSHI_QUANTITY"),
        }
    }
}

#[register_algorithm(default)]
impl Algorithm for CostAveraging {
    fn name(&self) -> &str {
        "CostAveraging"
    }
    fn description(&self) -> &str {
        "Finds the cheapest market BTC/USDT price, then places a Limit order at that price"
    }
    fn preferences(&self) -> Preferences {
        Preferences {
            cex: CexPreferences {
                max_network_delay_millis: 5000,
                rounding: CexRoundingPreferences {
                    activation_price_triggered_above: RoundingStrategy::AwayFromZero,
                    activation_price_triggered_below: RoundingStrategy::ToZero,
                    price: RoundingStrategy::ToZero,
                    quantity: RoundingStrategy::ToZero,
                    callback_rate_bps: RoundingStrategy::ToZero,
                },
            },
        }
    }
    fn signals(&self, c: &SignalContext) -> Signals {
        let mut signals = Signals::new();
        let one_millionth = 1.0 / 1_000_000.0;
        signals.write(&self.key_satoshi_quantity, one_millionth);
        let iter = c.cex_markets_for(AssetId::Bitcoin, AssetId::TetherUSD);
        let min_by_last_ask = iter.min_by(|(_a_exch, a_market), (_b_exch, b_market)| {
            let a_last_ask = a_market.ticks.ticks[0].ask.price;
            let b_last_ask = b_market.ticks.ticks[0].ask.price;
            a_last_ask.partial_cmp(&b_last_ask).unwrap()
        });
        if let Some((cheapest_cex_name, cheapest_market)) = min_by_last_ask {
            signals.write(&self.key_cex, cheapest_cex_name);
            signals.write(&self.key_market_exists, true);
            let cheapest_price = cheapest_market.ticks.ticks[0].ask.price / 1_000_000.0;
            signals.write(&self.key_cheapest_price, cheapest_price);
        }
        signals
    }
    fn strategy(&self, c: &StrategyContext) -> Command {
        let cex = c.signals.cex_id(&self.key_cex);
        let account = c.signals.account_id(&self.key_account);
        let btc = c.literals.asset_id(AssetId::Bitcoin);
        let usdt = c.literals.asset_id(AssetId::TetherUSD);
        let cheapest_price = c.signals.number(&self.key_cheapest_price);
        let satoshi_quantity = c.signals.number(&self.key_satoshi_quantity);
        let tag = Tag::new("CostAveraging");
        c.commands.if_else(
            c.conditions.signal(&self.key_market_exists),
            c.commands.if_else(
                c.conditions.compare(
                    c.portfolio.asset_available_in_cex_account(
                        usdt.clone(),
                        cex.clone(),
                        account.clone(),
                    ),
                    Ordering::Greater,
                    cheapest_price.clone(),
                ),
                c.commands.plan(vec![c.actions.send_order_request(
                    cex.clone(),
                    account.clone(),
                    c.orders.limit(
                        btc,
                        usdt.clone(),
                        Side::Buy,
                        Activation::Immediate,
                        cheapest_price,
                        TimeInForce::ImmediateOrCancel,
                        Quantity::OfBase(satoshi_quantity),
                        tag,
                    ),
                    RecoveryPolicy::with_default_response(ActionErrorResponse::Stop).on_error(
                        ActionErrorCause::TemporaryCexRejection,
                        ActionErrorResponse::Retry { max_retries: 3 },
                    ),
                )]),
                c.commands.no_op(),
            ),
            c.commands.no_op(),
        )
    }
}
