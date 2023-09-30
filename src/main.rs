#[macro_use]
extern crate rocket;
extern crate mysql;

use dotenv::dotenv;
use plogger;
use rand::Rng;
use rocket::form::{validate, Form};
use rocket::futures::{FutureExt, StreamExt};
use rocket::http::CookieJar;
use rocket::http::Status;
use rocket::response::Redirect;
use rocket_db_pools::sqlx::mysql::{MySqlQueryResult, MySqlRow};
use rocket_db_pools::sqlx::pool::PoolConnection;
use rocket_db_pools::sqlx::{Acquire, MySql, MySqlConnection, Row};
use rocket_db_pools::{sqlx, Connection, Database};
use rocket_dyn_templates::Template;
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

use serde::Serialize;

mod auth;

#[derive(Database)]
#[database("db")]
struct Db(sqlx::MySqlPool);

#[get("/")]
fn index() -> Template {
    let context: HashMap<String, String> = HashMap::new();
    Template::render("index", &context)
}

#[derive(Serialize)]
struct HostDashboardContext {
    authenticated: bool,
    quizzes: Vec<Quiz>,
}

#[get("/host")]
async fn host_dashboard(cookies: &CookieJar<'_>, mut db: Connection<Db>) -> Template {
    if let Some(_) = cookies.get_private("host_authenticated") {
        let rows = sqlx::query("SELECT * FROM quizzes")
            .fetch_all(&mut *db)
            .await
            .unwrap();

        let mut vec: Vec<Quiz> = Vec::new();

        for row in rows {
            let id: String = row.get(0);
            let code: u16 = row.get(1);
            let date: String = row.get(2);

            let quiz = Quiz {
                id: Uuid::from_str(&id).unwrap(),
                quiz_code: code,
                date: date,
            };

            vec.push(quiz);
        }

        log::debug!("{:?}", vec);

        let context = HostDashboardContext {
            authenticated: true,
            quizzes: vec,
        };
        Template::render("host_dashboard", &context)
    } else {
        let context = HostDashboardContext {
            authenticated: false,
            quizzes: Vec::new(),
        };
        Template::render("host_dashboard", &context)
    }
}

#[derive(FromForm)]
struct NewQuizForm {
    date: String, // TODO: use a date-specific type
}

#[derive(Debug, Serialize)]
pub struct Quiz {
    pub id: Uuid,
    pub quiz_code: u16,
    pub date: String, // Consider using a date/time type appropriate for your database.
}

#[post("/host/create_quiz", data = "<new_quiz_form>")]
async fn create_quiz(
    new_quiz_form: Form<NewQuizForm>,
    db: Connection<Db>,
) -> Result<Redirect, Status> {
    let quiz = Quiz::new(new_quiz_form.date.clone(), db).await;

    log::debug!("{:?}", quiz);

    Ok(Redirect::to("/host"))
}

#[derive(Serialize)]
struct ViewQuizAsHostContext {
    uuid: String,
    quiz: Quiz,
    questions: Vec<Question>,
}

impl Quiz {
    pub async fn new(date: String, mut db: Connection<Db>) -> Self {
        let quiz_id = Uuid::new_v4(); // Generate a UUID
        let quiz_code: u16 = rand::thread_rng().gen_range(1000..9999); // Generate a 4-digit code

        let quiz = Quiz {
            id: quiz_id,
            quiz_code,
            date: date,
        };

        quiz.persist(db).await.unwrap();

        log::debug!("Quiz {} was created", quiz.id.as_hyphenated().to_string());

        quiz
    }

    async fn persist(&self, mut db: Connection<Db>) -> Result<(), String> {
        let query = r"
            INSERT INTO quizzes (id, quiz_code, date)
            VALUES (?, ?, ?)";

        match sqlx::query(query)
            .bind(self.id.as_hyphenated().to_string())
            .bind(self.quiz_code)
            .bind(&self.date)
            .execute(&mut *db)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("{:?}", e);
                Err(e.to_string())
            }
        }
    }

    pub async fn find_by_uuid(uuid: Uuid, db: &mut PoolConnection<MySql>) -> Result<Quiz, String> {
        let query = "SELECT * FROM quizzes WHERE id = ? LIMIT 1";

        let result = sqlx::query(query)
            .bind(uuid.to_string())
            .fetch_one(db)
            .await
            .unwrap();

        Ok(Quiz::from_mysql_row(result))
    }

    pub fn from_mysql_row(row: MySqlRow) -> Self {
        let id = row.get(0);
        Quiz {
            id: Uuid::from_str(id).unwrap(),
            quiz_code: row.get(1),
            date: row.get(2),
        }
    }
}

#[get("/host/quiz/<uuid>")]
async fn view_quiz_as_host(uuid: &str, mut db: Connection<Db>) -> Template {
    let validated_uuid = Uuid::from_str(uuid).unwrap();

    let quiz = Quiz::find_by_uuid(validated_uuid, &mut *db).await.unwrap();
    let questions = Question::find_all_for_quiz(validated_uuid, &mut *db).await;

    let context = ViewQuizAsHostContext {
        uuid: String::from(uuid),
        quiz: quiz,
        questions: questions,
    };
    Template::render("edit_quiz", &context)
}

#[derive(FromForm)]
struct NewQuestionForm {
    question: String,
    answer: String,
    section: i32,
}

#[post("/host/quiz/<uuid>/questions", data = "<new_question_form>")]
async fn add_question_to_quiz(
    uuid: &str,
    new_question_form: Form<NewQuestionForm>,
    db: Connection<Db>,
) -> Result<Redirect, Status> {
    let validated_uuid = Uuid::from_str(uuid).unwrap();

    let question = Question::create_for_quiz(
        validated_uuid,
        String::from(&new_question_form.question),
        String::from(&new_question_form.answer),
        db,
    )
    .await
    .unwrap();

    log::debug!("Question created: {:?}", question);

    Ok(Redirect::to(format!("/host/quiz/{}", uuid)))
}

#[derive(Debug, Serialize)]
struct Question {
    id: Uuid,
    quiz_id: Uuid,
    question: String,
    answer: String,
    section: u32,
}

impl Question {
    pub async fn find_all_for_quiz(quiz_uuid: Uuid, db: &mut PoolConnection<MySql>) -> Vec<Self> {
        let query = "SELECT * FROM questions WHERE quiz_id = ?";
        let rows = sqlx::query(query)
            .bind(quiz_uuid.as_hyphenated().to_string())
            .fetch_all(&mut *db)
            .await
            .unwrap();

        let mut vec: Vec<Question> = Vec::new();

        for row in rows {
            let question = Question {
                id: Uuid::from_str(row.get(0)).unwrap(),
                quiz_id: Uuid::from_str(row.get(1)).unwrap(),
                question: row.get(2),
                answer: row.get(3),
                section: row.get(4),
            };

            vec.push(question);
        }

        log::debug!("{:?}", vec);

        vec
    }

    pub async fn create_for_quiz(
        quiz_uuid: Uuid,
        question: String,
        answer: String,
        mut db: Connection<Db>,
    ) -> Result<Self, String> {
        let uuid = Uuid::new_v4();
        let question = Question {
            id: uuid,
            quiz_id: quiz_uuid,
            question: question,
            answer: answer,
            section: 1,
        };

        let query = r"
        INSERT INTO questions (id, quiz_id, question, answer, section)
        VALUES (?, ?, ?, ?, ?)";

        match sqlx::query(query)
            .bind(&question.id.as_hyphenated().to_string())
            .bind(&question.quiz_id.as_hyphenated().to_string())
            .bind(&question.question)
            .bind(&question.answer)
            .bind(&question.section)
            .execute(&mut *db)
            .await
        {
            Ok(_) => {}
            Err(e) => {
                log::error!("{:?}", e);
                panic!()
            }
        }

        Ok(question)
    }
}

#[launch]
fn rocket() -> _ {
    dotenv().ok();

    plogger::init(true);

    rocket::build()
        .attach(Template::fairing())
        .attach(Db::init())
        .mount(
            "/",
            routes![
                index,
                host_dashboard,
                view_quiz_as_host,
                create_quiz,
                add_question_to_quiz,
                auth::authenticate
            ],
        )
}
