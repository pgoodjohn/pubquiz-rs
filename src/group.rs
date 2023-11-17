use rocket::form::Form;
use rocket::http::Cookie;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket_db_pools::sqlx;
use rocket_db_pools::sqlx::pool::PoolConnection;
use rocket_db_pools::sqlx::MySql;
use rocket_db_pools::Connection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

use crate::Db;

#[derive(FromForm, Debug)]
pub struct GroupSignupForm {
    pub name: String,
}

#[post(
    "/quiz/<uuid>/signup",
    format = "application/x-www-form-urlencoded",
    data = "<form>"
)]
pub async fn signup(
    form: Form<GroupSignupForm>,
    uuid: &str,
    cookies: &CookieJar<'_>,
    mut db: Connection<Db>,
) -> Redirect {
    let validated_uuid = Uuid::from_str(uuid).unwrap();
    let quiz = super::Quiz::find_by_uuid(validated_uuid, &mut *db)
        .await
        .unwrap();

    let group = Group::new(&quiz, &form.name, &mut *db).await;

    cookies.add_private(Cookie::new("registered_group", group.to_string()));

    Redirect::to(format!("/quiz/{}", quiz.quiz_code))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub quiz_id: Uuid,
    pub name: String,
}

impl Group {
    pub async fn new(quiz: &super::Quiz, name: &str, db: &mut PoolConnection<MySql>) -> Self {
        let group = Group {
            id: Uuid::new_v4(),
            quiz_id: quiz.id.clone(),
            name: String::from(name),
        };

        group.persist(db).await;

        group
    }

    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    async fn persist(&self, db: &mut PoolConnection<MySql>) -> Result<(), String> {
        let query = r"
            INSERT INTO `groups` (id, quiz_id, name)
            VALUES (?, ?, ?)";

        match sqlx::query(query)
            .bind(self.id.as_hyphenated().to_string())
            .bind(self.quiz_id.as_hyphenated().to_string())
            .bind(&self.name)
            .execute(db)
            .await
        {
            Ok(_) => {
                log::debug!("Stored group {:?}", self);
                Ok(())
            }
            Err(e) => {
                log::error!("{:?}", e);
                Err(e.to_string())
            }
        }
    }

    pub fn from_cookie(cookie: Cookie) -> Self {
        let value = cookie.value();

        let group: Group = serde_json::from_str(value).unwrap();

        group
    }
}
