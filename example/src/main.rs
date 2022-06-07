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

use flagsmith::{Flag, Flagsmith, FlagsmithOptions};
use flagsmith_flag_engine::identities::Trait;
use flagsmith_flag_engine::types::{FlagsmithValue, FlagsmithValueType};

#[derive(Serialize)]
struct TemplateContext {
    show_button: bool,
    button_colour: String,
    identifier: String,
}
fn default_flag_handler(feature_name: &str) -> Flag {
    let mut flag: Flag = Default::default();
    if feature_name == "secret_button" {
        flag.value.value_type = FlagsmithValueType::String;
        flag.value.value = serde_json::json!({"colour": "#b8b8b8"}).to_string();
    }
    return flag;
}

#[get("/?<identifier>&<trait_key>&<trait_value>")]
fn home(
    identifier: Option<String>,
    trait_key: Option<String>,
    trait_value: Option<String>,
) -> Template {
    let options = FlagsmithOptions {
        default_flag_handler: Some(default_flag_handler),
        enable_local_evaluation: true,
        ..Default::default()
    };

    let flagsmith = Flagsmith::new(
        env::var("FLAGSMITH_ENVIRONMENT_KEY")
            .expect("FLAGSMITH_ENVIRONMENT_KEY not found in environment"),
        options,
    );
    let flags;
    if identifier.is_some() {
        let traits = match trait_key {
            Some(trait_key) if trait_key != "".to_string() => Some(vec![Trait {
                trait_key,
                trait_value: FlagsmithValue {
                    value: trait_value.unwrap_or("".to_string()),
                    value_type: FlagsmithValueType::None,
                },
            }]),
            Some(_) => None,
            None => None,
        };
        flags = flagsmith
            .get_identity_flags(identifier.as_ref().unwrap(), traits)
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
