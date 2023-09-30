use rocket::http::CookieJar;
use rocket::http::Cookie;
use rocket::form::Form;
use rocket::response::Redirect;
use std::env;

#[derive(FromForm, Debug)]
pub struct LoginForm {
    pub password: String,
}

#[post("/host/authenticate", format = "application/x-www-form-urlencoded", data = "<form>")]
pub fn authenticate(form: Form<LoginForm>, cookies: &CookieJar<'_>) -> Redirect {
    let host_password = env::var("HOST_PASSWORD").expect("Password not set");
    let password = String::from(&form.password);
    if password == host_password {
        log::debug!("Authenticated! Setting cookie");
        cookies.add_private(Cookie::new("host_authenticated", "true"));
    } else {
        log::debug!("Authentication failed. Input: {} - Password: {}", password, host_password);
    }

    Redirect::to("/host")
}
