use chrono::{DateTime, Local};
use mqtt::topics::Contrat;
use tarifs_cre::PricePeriod;

pub struct TarifsDb {
    tarifs: Vec<PricePeriod>,
}

impl TarifsDb {
    pub async fn fetch_for_contrat(contrat: &Contrat) -> anyhow::Result<Self> {
        let mut tarifs = tarifs_cre::fetch_price_periods(contrat).await?;
        tarifs.sort_by_key(|tp| tp.start);
        for i in 1..tarifs.len() {
            if tarifs[i].start < tarifs[i - 1].end {
                log::warn!(
                    "Fixing overlapping price periods: {:?} later than {:?}",
                    tarifs[i - 1],
                    tarifs[i]
                );
            }
            if tarifs[i].start > tarifs[i - 1].end {
                log::warn!(
                    "Fixing gap between price periods: {:?} to {:?}",
                    tarifs[i - 1],
                    tarifs[i]
                );
            }
            tarifs[i - 1].end = tarifs[i].start;
        }
        Ok(Self { tarifs })
    }

    pub fn get_price_periods_for_time_span(
        &self,
        start: DateTime<Local>,
        end: DateTime<Local>,
    ) -> Vec<PricePeriod> {
        let mut result = Vec::new();
        for tp in &self.tarifs {
            if period_overlaps(start, end, tp.start, tp.end) {
                result.push(tp.clone());
            }
        }
        result
    }
}

fn period_overlaps(
    start1: DateTime<Local>,
    end1: DateTime<Local>,
    start2: DateTime<Local>,
    end2: DateTime<Local>,
) -> bool {
    start1 < end2 && start2 < end1
}
