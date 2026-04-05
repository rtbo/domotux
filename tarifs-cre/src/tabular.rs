//! Support for tabular data API of data.gouv.fr

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Resource<'a, Row> {
    id: &'a str,
    client: reqwest::Client,
    _phantom: std::marker::PhantomData<Row>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Links {
    next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Response<Row> {
    data: Vec<Row>,
    links: Links,
}

impl<'a, Row> Resource<'a, Row> {
    pub fn new(id: &'a str) -> Self {
        Self {
            id,
            client: reqwest::Client::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn data_url(&self) -> String {
        format!(
            "https://tabular-api.data.gouv.fr/api/resources/{}/data/",
            self.id
        )
    }

    pub async fn fetch_all(&self) -> anyhow::Result<Vec<Row>>
    where
        Row: serde::de::DeserializeOwned,
    {
        let mut all_rows = Vec::new();
        let query = &[
            ("page", 1),
            ("page_size", 200), // Max page size is 200
        ];
        let mut response = self.do_fetch(&self.data_url(), query).await?;
        all_rows.extend(response.data);

        while let Some(next) = response.links.next {
            response = self.do_fetch(&next, &[]).await?;
            all_rows.extend(response.data);
        }

        Ok(all_rows)
    }

    async fn do_fetch(&self, url: &str, query: &[(&str, usize)]) -> anyhow::Result<Response<Row>>
    where
        Row: serde::de::DeserializeOwned,
    {
        let req = self.client.get(url).query(query);
        let res = req.send().await?;

        let response = res.json::<Response<Row>>().await?;
        Ok(response)
    }
}
