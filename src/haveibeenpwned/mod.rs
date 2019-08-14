use crate::PasswordHashEntry;
use crypto::digest::Digest;
use crypto::sha1::Sha1;
use log::error;
use std::collections::HashMap;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Error};

/// The possible errors which can occur on instantiation of the [HaveIBeenPwnedParser](struct.HaveIBeenPwnedParser.html) class.
#[derive(Debug)]
pub enum CreateInstanceError {
    /// It seems that the format of the file is not as expected.
    Format(FormatErrorKind),
    /// There was a generic IO error.
    Io(Error),
}

/// The more specific error if the format could not be read.
#[derive(Debug)]
pub enum FormatErrorKind {
    /// It seems that the file is not a plain text file.
    NotATextFile,
    /// It seems that the format of at least one of the lines in the file is invalid.
    LineFormatNotCorrect,
}

impl FormatErrorKind {
    fn to_string(&self) -> &str {
        match *self {
            FormatErrorKind::NotATextFile => "not a text file which can be parsed",
            FormatErrorKind::LineFormatNotCorrect => {
                "format of lines does not match the required format"
            }
        }
    }
}

impl Display for CreateInstanceError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match *self {
            CreateInstanceError::Format(ref err_kind) => {
                write!(f, "Format error: {}", err_kind.to_string())
            }
            CreateInstanceError::Io(ref err) => write!(f, "IO error: {}", err),
        }
    }
}

/// This class can be used to parse the password files provided by https://haveibeenpwned.com.
pub struct HaveIBeenPwnedParser {
    known_password_hashes: Option<HashMap<String, u64>>,
    file_size: u64,
    password_file: Option<BufReader<File>>,
}

impl HaveIBeenPwnedParser {
    /// Get a new instance of the file parsed based on the provided file path.
    ///
    /// # Errors
    ///
    /// This function will return an error in the following situations, but is not
    /// limited to just these cases:
    ///
    ///  * The user does not have the access rights to access the provided file.
    ///  * The file does not exist.
    ///  * The file is not a plain text file.
    ///  * The format of the lines in the text file does not match the required format.
    ///
    /// # Example
    /// ```
    /// use pwned_rs::haveibeenpwned::HaveIBeenPwnedParser;
    ///
    /// match HaveIBeenPwnedParser::from_file("/path/to/the/hash/file.txt") {
    ///     Ok(instance) => println!("Got an instance of the file parser!"),
    ///     Err(error) => println!("Could not get an instance, the error was: {}", error)
    /// }
    /// ```
    pub fn from_file(path_to_file: &str) -> Result<HaveIBeenPwnedParser, CreateInstanceError> {
        // be sure that the file exists, if not we should return a proper error which the caller can deal with
        let file_meta_data = match std::fs::metadata(path_to_file) {
            Ok(data) => data,
            Err(error) => return Err(CreateInstanceError::Io(error)),
        };

        // try to figure our how many entries are stored in the file
        let file_reader = match OpenOptions::new()
            .append(false)
            .create(false)
            .read(true)
            .open(&path_to_file)
        {
            Ok(file_handle) => BufReader::new(file_handle),
            Err(error) => return Err(CreateInstanceError::Io(error)),
        };

        // return the successfully created instance of the parser
        Ok(HaveIBeenPwnedParser {
            known_password_hashes: None,
            password_file: Some(file_reader),
            file_size: file_meta_data.len(),
        })
    }

    /// Get the number of occurrences of a password according to the loaded hash file.
    ///
    /// # Example
    /// ```
    /// use pwned_rs::haveibeenpwned::HaveIBeenPwnedParser;
    ///
    /// match HaveIBeenPwnedParser::from_file("/path/to/the/hash/file.txt") {
    ///     Ok(instance) => {
    ///         let number_of_occurrences = instance.get_usage_count("password");
    ///         println!("The password 'password' was used {} times", number_of_occurrences);
    ///     },
    ///     Err(error) => println!("Could not get an instance, the error was: {}", error)
    /// }
    /// ```
    pub fn get_usage_count(&self, password: &str) -> u64 {
        match self.known_password_hashes {
            Some(ref hash_map) => {
                // get the SHA-1 hashed password
                let mut hasher = Sha1::new();
                hasher.input_str(password);
                let password_hash = hasher.result_str();

                // return the number of occurrences in the hash map
                match hash_map.get(password_hash.as_str()) {
                    Some(number) => *number,
                    None => 0,
                }
            }
            None => 0,
        }
    }

    /// Get the size of the original password file.
    ///
    /// # Example
    /// ```
    /// use pwned_rs::haveibeenpwned::HaveIBeenPwnedParser;
    ///
    /// match HaveIBeenPwnedParser::from_file("/path/to/the/hash/file.txt") {
    ///     Ok(instance) => {
    ///         let file_size = match instance.get_file_size() {
    ///             Some(size) => size,
    ///             None => panic!("It seems that the instance of this object was not created using a file."),
    ///         };
    ///         println!("The original password file is {} bytes long", file_size);
    ///     },
    ///     Err(error) => println!("Could not get an instance, the error was: {}", error)
    /// }
    /// ```
    pub fn get_file_size(&self) -> Option<u64> {
        if self.password_file.is_some() {
            return Some(self.file_size);
        }
        None
    }
}

impl Iterator for HaveIBeenPwnedParser {
    type Item = PasswordHashEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // be sure that we are running in file mode, otherwise we can return immediately
        let password_file_reader = match &mut self.password_file {
            Some(reader) => reader,
            None => return None,
        };

        // get the next line from the file
        let mut entry_line = String::new();
        let line_length = match password_file_reader.read_line(&mut entry_line) {
            Ok(length) => length,
            Err(_) => return None,
        };

        //
        let mut entry_splitted = entry_line.trim().split(':');

        //
        let password_hash = match entry_splitted.next() {
            Some(key_text) => key_text.to_lowercase(),
            None => {
                error!("Could not get the password hash part of the entry!");
                return None;
            }
        };

        // try to get the number of occurrences of the password hash
        let occurrences = match entry_splitted.next() {
            Some(value_text) => match value_text.parse::<u64>() {
                Ok(value_as_int) => value_as_int,
                Err(_) => {
                    error!("Could not parse the number of occurrences of the password. Maybe \"{}\" not a number.", value_text);
                    return None;
                }
            },
            None => {
                error!("Could not get the occurrence count.");
                return None;
            }
        };

        // return the parsed password entry
        Some(PasswordHashEntry {
            hash: password_hash,
            occurrences,
            entry_size: line_length as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_instance_with_invalid_path_fails() {
        let maybe_instance = HaveIBeenPwnedParser::from_file("/this/file/does/not/exist.txt");

        assert_eq!(true, maybe_instance.is_err());
        let error = maybe_instance.err().unwrap();
        assert_eq!(true, error.to_string().contains("IO error:"));
    }
}
