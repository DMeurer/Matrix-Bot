use anyhow::Result;
use regex::Regex;
use scraper::{Html, Selector};

#[derive(Debug, Clone, Default)]
pub struct Meal {
    pub name: String,
    pub allergens: String,
    pub additives: String,
    pub price_students: String,
    pub price_employees: String,
    pub price_guests: String,
}

#[derive(Debug, Default)]
pub struct Meals {
    pub monday: Vec<Meal>,
    pub tuesday: Vec<Meal>,
    pub wednesday: Vec<Meal>,
    pub thursday: Vec<Meal>,
    pub friday: Vec<Meal>,
    pub saturday: Vec<Meal>,
}

pub fn clean_text(text: &str) -> String {
    let text = text.replace('\n', " ").replace('\t', " ");
    Regex::new(r"\s+")
        .unwrap()
        .replace_all(&text, " ")
        .trim()
        .to_string()
}

pub fn camel_case_to_normal(s: &str) -> String {
    Regex::new(r"([A-Za-z][^A-Z]*|\d?\.?\d+ ?€?)")
        .unwrap()
        .find_iter(s)
        .map(|m| m.as_str().to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_meal(text: &str) -> Meal {
    let mut meal = Meal::default();

    let (rest, allergens) = text
        .split_once("enthält Allergene: ")
        .map(|(b, a)| (b, a.trim().to_string()))
        .unwrap_or((text, "Keine".to_string()));
    meal.allergens = camel_case_to_normal(&allergens);

    let (rest, additives) = rest
        .split_once("Kennzeichnungen/Zusatzstoffe: ")
        .map(|(b, a)| (b, a.trim().to_string()))
        .unwrap_or((rest, "Keine".to_string()));
    meal.additives = camel_case_to_normal(&additives);

    let (rest, price_guests) = rest
        .split_once("Gäste")
        .map(|(b, a)| (b, a.trim().to_string()))
        .unwrap_or((rest, "Keine Angabe".to_string()));
    meal.price_guests = camel_case_to_normal(&price_guests);

    let (rest, price_employees) = rest
        .split_once("Beschäftigte")
        .map(|(b, a)| (b, a.trim().to_string()))
        .unwrap_or((rest, "Keine Angabe".to_string()));
    meal.price_employees = camel_case_to_normal(&price_employees);

    let (name_raw, price_students) = rest
        .split_once("Studierende, Schüler")
        .map(|(b, a)| (b, a.trim().to_string()))
        .unwrap_or((rest, "Keine Angabe".to_string()));
    meal.price_students = camel_case_to_normal(&price_students);

    let name = Regex::new(r"Essen \d ").unwrap().replace_all(name_raw, " ");
    let name = Regex::new(r"Preise \+ Kennzeichnungen")
        .unwrap()
        .replace_all(&name, " ");
    meal.name = camel_case_to_normal(name.trim());

    meal
}

pub async fn load_meals() -> Result<Meals> {
    let body = reqwest::get(
        "https://www.swfr.de/essen/mensen-cafes-speiseplaene/mensa-furtwangen",
    )
    .await?
    .text()
    .await?;

    parse_html_meals(&body)
}

fn parse_html_meals(body: &str) -> Result<Meals> {
    let document = Html::parse_document(body);
    let meal_container = ".col-span-1.bg-lighter-cyan.py-20px.px-15px.flex.flex-col";
    let text_node = "small.extra-text.mb-15px";

    let mut meals = Meals::default();

    for (tab_id, day_meals) in [
        ("tab-mon", &mut meals.monday as &mut Vec<Meal>),
        ("tab-tue", &mut meals.tuesday),
        ("tab-wed", &mut meals.wednesday),
        ("tab-thu", &mut meals.thursday),
        ("tab-fri", &mut meals.friday),
        ("tab-sat", &mut meals.saturday),
    ] {
        let selector_str = format!("div#{tab_id} {meal_container} {text_node}");
        let selector = Selector::parse(&selector_str)
            .map_err(|e| anyhow::anyhow!("Invalid CSS selector: {e:?}"))?;

        for element in document.select(&selector) {
            let text: String = element.text().collect();
            day_meals.push(parse_meal(&clean_text(&text)));
        }
    }

    Ok(meals)
}

pub fn format_meals(meals: &Meals, days: &[usize]) -> String {
    let mut message = String::from("Mensa Furtwangen\n\n");

    for &day in days {
        let (day_name, day_meals): (&str, &Vec<Meal>) = match day {
            1 => ("Montag", &meals.monday),
            2 => ("Dienstag", &meals.tuesday),
            3 => ("Mittwoch", &meals.wednesday),
            4 => ("Donnerstag", &meals.thursday),
            5 => ("Freitag", &meals.friday),
            6 => ("Samstag", &meals.saturday),
            _ => continue,
        };

        message.push_str(day_name);
        message.push('\n');
        for meal in day_meals {
            message.push_str(&format!("    {}\n", meal.name));
            message.push_str(&format!(
                "    Preis Studierende: {}\n\n",
                meal.price_students
            ));
        }
    }

    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_collapses_whitespace() {
        assert_eq!(clean_text("  foo\n\tbar  "), "foo bar");
    }

    #[test]
    fn camel_case_splits_words() {
        let result = camel_case_to_normal("GemüsebologneSojaschnetzel");
        assert!(result.contains("gemüsebologne"));
        assert!(result.contains("sojaschnetzel"));
    }

    #[test]
    fn camel_case_preserves_price() {
        let result = camel_case_to_normal("2.50 €");
        assert_eq!(result, "2.50 €");
    }

    #[test]
    fn parse_meal_text_full() {
        let text = "Essen 1 GemüsebologneSojaschnetzel Preise + Kennzeichnungen \
            Studierende, Schüler 2.50 € Beschäftigte 3.50 € Gäste 4.50 € \
            Kennzeichnungen/Zusatzstoffe: A, B enthält Allergene: Gluten, Soja";
        let meal = parse_meal(text);
        assert!(!meal.name.is_empty());
        assert!(meal.price_students.contains("2.50"));
        assert!(meal.allergens.contains("gluten"));
        assert!(meal.allergens.contains("soja"));
    }

    #[test]
    fn parse_meal_text_no_allergens() {
        let text = "Essen 1 TestEssen Preise + Kennzeichnungen \
            Studierende, Schüler 2.50 € Beschäftigte 3.50 € Gäste 4.50 € \
            Kennzeichnungen/Zusatzstoffe: Keine";
        let meal = parse_meal(text);
        assert_eq!(meal.allergens, "keine");
    }

    #[test]
    fn format_empty_day() {
        let meals = Meals::default();
        let result = format_meals(&meals, &[1]);
        assert!(result.contains("Mensa Furtwangen"));
        assert!(result.contains("Montag"));
    }

    #[test]
    fn format_meal_basic() {
        let mut meals = Meals::default();
        meals.monday.push(Meal {
            name: "test essen".to_string(),
            price_students: "2.50 €".to_string(),
            ..Default::default()
        });
        let result = format_meals(&meals, &[1]);
        assert!(result.contains("test essen"));
        assert!(result.contains("2.50 €"));
    }
}
