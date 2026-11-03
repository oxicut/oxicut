//! What the user knows about a picture: when it was taken, where, what it shows and who is in it.
//! These are written into the exported file (EXIF and XMP, see `metadata` and `xmp`), never into
//! the scan.

/// Longest caption, in characters.
pub const MAX_CAPTION: usize = 2000;
/// Longest date wording or place, and longest name of a person.
pub const MAX_TEXT: usize = 200;
/// Most people named on one picture.
pub const MAX_PEOPLE: usize = 100;

/// The details of one picture. The date is kept as the user wrote it (`date_text`) next to what could
/// be read out of it (`year`, `month`, `day`); `approximate` says the wording was "circa 1980" or alike.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Details {
    pub date_text: String,
    pub year: Option<u16>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub approximate: bool,
    pub place: String,
    pub caption: String,
    pub people: Vec<String>,
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
            if leap { 29 } else { 28 }
        }
    }
}

/// Trim, drop control characters (a NUL would end an EXIF string early) and cut to `max` characters.
fn clean(text: &str, max: usize, keep_newlines: bool) -> String {
    let kept: String = text
        .chars()
        .filter(|c| !c.is_control() || (keep_newlines && (*c == '\n' || *c == '\r' || *c == '\t')))
        .collect();
    kept.trim().chars().take(max).collect::<String>().trim().to_string()
}

impl Details {
    /// The same details with every text cleaned and cut to its limit, the date numbers checked
    /// (a month needs a year, a day needs a month, nothing out of range) and the people listed once each.
    pub fn normalized(&self) -> Details {
        let year = self.year.filter(|y| (1..=9999).contains(y));
        let month = year.and(self.month).filter(|m| (1..=12).contains(m));
        let day = month
            .zip(year)
            .and_then(|(m, y)| self.day.filter(|d| *d >= 1 && *d <= days_in_month(y, m)));
        let mut people: Vec<String> = Vec::new();
        for name in &self.people {
            let name = clean(name, MAX_TEXT, false);
            if name.is_empty() || people.iter().any(|p| p.to_lowercase() == name.to_lowercase()) {
                continue;
            }
            if people.len() < MAX_PEOPLE {
                people.push(name);
            }
        }
        Details {
            date_text: clean(&self.date_text, MAX_TEXT, false),
            year,
            month,
            day,
            approximate: self.approximate && year.is_some(),
            place: clean(&self.place, MAX_TEXT, false),
            caption: clean(&self.caption, MAX_CAPTION, true),
            people,
        }
    }

    /// Nothing to write.
    pub fn is_empty(&self) -> bool {
        self.date_text.is_empty()
            && self.year.is_none()
            && self.place.is_empty()
            && self.caption.is_empty()
            && self.people.is_empty()
    }

    /// The date as far as it is known: `1975`, `1975-06` or `1975-06-14` (what XMP allows).
    pub fn iso_date(&self) -> Option<String> {
        let year = self.year?;
        Some(match (self.month, self.day) {
            (Some(m), Some(d)) => format!("{year:04}-{m:02}-{d:02}"),
            (Some(m), None) => format!("{year:04}-{m:02}"),
            _ => format!("{year:04}"),
        })
    }

    /// The date as EXIF wants it. EXIF has no partial dates, so a missing month or day is January or
    /// the 1st: the exact wording and precision are kept in the XMP next to it.
    pub fn exif_date(&self) -> Option<String> {
        let year = self.year?;
        Some(format!(
            "{year:04}:{:02}:{:02} 00:00:00",
            self.month.unwrap_or(1),
            self.day.unwrap_or(1)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_dates_are_written_as_far_as_they_are_known() {
        let mut d = Details {
            year: Some(1975),
            ..Default::default()
        };
        assert_eq!(d.iso_date().as_deref(), Some("1975"));
        assert_eq!(d.exif_date().as_deref(), Some("1975:01:01 00:00:00"));
        d.month = Some(6);
        assert_eq!(d.iso_date().as_deref(), Some("1975-06"));
        d.day = Some(14);
        assert_eq!(d.iso_date().as_deref(), Some("1975-06-14"));
        assert_eq!(d.exif_date().as_deref(), Some("1975:06:14 00:00:00"));
        assert_eq!(Details::default().iso_date(), None);
    }

    #[test]
    fn impossible_dates_fall_back_to_what_is_still_possible() {
        let n = |year, month, day| {
            Details {
                year,
                month,
                day,
                ..Default::default()
            }
            .normalized()
        };
        assert_eq!(n(Some(1975), Some(13), Some(5)).iso_date().as_deref(), Some("1975"));
        assert_eq!(n(Some(1975), Some(2), Some(30)).iso_date().as_deref(), Some("1975-02"));
        assert_eq!(n(Some(1976), Some(2), Some(29)).iso_date().as_deref(), Some("1976-02-29"));
        assert_eq!(n(Some(1900), Some(2), Some(29)).iso_date().as_deref(), Some("1900-02"));
        // A month without a year is nothing.
        assert_eq!(n(None, Some(6), Some(1)).iso_date(), None);
        assert_eq!(n(Some(0), None, None).iso_date(), None);
    }

    #[test]
    fn texts_are_cleaned_cut_and_people_listed_once() {
        let d = Details {
            date_text: "  circa 1980\u{0} ".into(),
            place: "Balaton\n".into(),
            caption: "Line one\nLine two\u{0}".into(),
            people: vec![
                " Anna ".into(),
                "anna".into(),
                "".into(),
                "Péter".into(),
                "x".repeat(500),
            ],
            approximate: true,
            ..Default::default()
        }
        .normalized();
        assert_eq!(d.date_text, "circa 1980");
        assert_eq!(d.place, "Balaton");
        assert_eq!(d.caption, "Line one\nLine two");
        assert_eq!(d.people.len(), 3);
        assert_eq!(&d.people[..2], ["Anna", "Péter"]);
        assert_eq!(d.people[2].chars().count(), MAX_TEXT);
        // "Approximate" means nothing without a year.
        assert!(!d.approximate);

        let long = Details {
            caption: "é".repeat(MAX_CAPTION + 50),
            people: (0..150).map(|i| format!("Person {i}")).collect(),
            ..Default::default()
        }
        .normalized();
        assert_eq!(long.caption.chars().count(), MAX_CAPTION);
        assert_eq!(long.people.len(), MAX_PEOPLE);
    }

    #[test]
    fn nothing_typed_is_empty() {
        assert!(Details::default().is_empty());
        assert!(Details { place: "   ".into(), ..Default::default() }.normalized().is_empty());
        assert!(!Details { caption: "x".into(), ..Default::default() }.is_empty());
    }
}
