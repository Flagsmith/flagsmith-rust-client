#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate serde_derive;
extern crate flagsmith;
extern crate rocket_contrib;
extern crate serde_json;

use std::env;

use rocket_contrib::templates::Template;

use flagsmith::{Flagsmith, FlagsmithOptions};
use flagsmith_flag_engine::identities::Trait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};

#[derive(Serialize)]
struct TemplateContext {
    show_button: bool,
    button_colour: String,
    identifier: String,
}

#[derive(Serialize)]
struct  Dumb{}
#[get("/?<identifier>&<trait_key>&<trait_value>")]
fn home(
    identifier: Option<String>,
    trait_key: Option<String>,
    trait_value: Option<String>,
) -> Template {

     let flagsmith = Flagsmith::new(
         env::var("ENVIRONMENT_KEY").unwrap(),
         FlagsmithOptions::default(),
     );
     let flags;
     if identifier.is_some() {
         let traits = match trait_key {
             Some(trait_key) => {
                 Some(vec![Trait {
                     trait_key,
                     trait_value: FlagsmithValue {
                         value: trait_value.unwrap_or("".to_string()),
                         value_type: FlagsmithValueType::String,
                     },
                 }])
             }
             None => None,
         };
         flags = flagsmith
             .get_identity_flags(identifier.as_ref().unwrap().to_string(), traits)
             .unwrap();
     } else {
         // Get the default flags for the current environment
         flags = flagsmith.get_environment_flags().unwrap();
     }

     let show_button = flags.is_feature_enabled("secret_button").unwrap();
     let button_data = flags.get_feature_value_as_string("secret_button").unwrap();

     let button_json: serde_json::Value = serde_json::from_str(&button_data).unwrap();
    let button_colour = button_json["colour"].as_str().unwrap().to_string();

    let context = TemplateContext {
        show_button,
        button_colour,
        identifier: identifier.unwrap_or("World".to_string()),
    };

    Template::render("home", &context)
}

fn rocket() -> rocket::Rocket {
    rocket::ignite()
        .mount("/", routes![home])
        .attach(Template::fairing())
}

fn main() {
    rocket().launch();
}
