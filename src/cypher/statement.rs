use std::convert::From;

/// Helper macro to simplify the creation of complex statements
///
/// Pass in the statement text as the first argument followed by the (optional) parameters, which
/// must be in the format `"param" => value` and wrapped in `{}`
///
/// # Examples
///
/// ```
/// # #[macro_use] extern crate rusted_cypher;
/// # fn main() {
/// // Without parameters
/// let statement = cypher_stmt!("MATCH n RETURN n");
/// // With parameters
/// let statement = cypher_stmt!("MATCH n RETURN n" {
///     "param1" => "value1",
///     "param2" => 2,
///     "param3" => 3.0
/// });
/// # }
/// ```
#[macro_export]
macro_rules! cypher_stmt {
    ( $s:expr ) => { $crate::Statement::new($s) };
    ( $s:expr { $( $k:expr => $v:expr ),+ } ) => {
        $crate::Statement::new($s)
            $(.with_param($k, $v))*
    }
}

#[cfg(not(feature = "rustc-serialize"))]
mod inner {
    use std::collections::BTreeMap;
    use serde::{Serialize, Deserialize};
    use serde_json::{self, Value};

    /// Represents a statement to be sent to the server
    #[derive(Clone, Debug, Serialize)]
    pub struct Statement {
        statement: String,
        parameters: BTreeMap<String, Value>,
    }

    impl Statement  {
        pub fn new(statement: &str) -> Self {
            Statement {
                statement: statement.to_owned(),
                parameters: BTreeMap::new(),
            }
        }

        /// Returns the statement text
        pub fn statement(&self) -> &str {
            &self.statement
        }

        /// Adds parameter in builder style
        ///
        /// This method consumes `self` and returns it with the parameter added, so the binding does
        /// not need to be mutable
        ///
        /// # Examples
        ///
        /// ```
        /// # use rusted_cypher::Statement;
        /// let statement = Statement::new("MATCH n RETURN n")
        ///     .with_param("param1", "value1")
        ///     .with_param("param2", 2)
        ///     .with_param("param3", 3.0);
        /// ```
        pub fn with_param<V: Serialize + Copy>(mut self, key: &str, value: V) -> Self {
            self.add_param(key, value);
            self
        }

        /// Adds parameter to the `Statement`
        pub fn add_param<V: Serialize + Copy>(&mut self, key: &str, value: V) {
            self.parameters.insert(key.to_owned(), serde_json::value::to_value(&value));
        }

        /// Gets the value of the parameter
        ///
        /// Returns `None` if there is no parameter with the given name or `Some(serde_json::error::Error)``
        /// if the parameter cannot be converted back from `serde_json::value::Value`
        pub fn param<V: Deserialize>(&self, key: &str) -> Option<Result<V, serde_json::error::Error>> {
            self.parameters.get(key.into()).map(|v| serde_json::value::from_value(v.clone()))
        }

        /// Use `Self::param`
        pub fn get_param<V: Deserialize>(&self, key: &str) -> Option<Result<V, serde_json::error::Error>> {
            self.param(key)
        }

        /// Gets a reference to the underlying parameters `BTreeMap`
        pub fn parameters(&self) -> &BTreeMap<String, Value> {
            &self.parameters
        }

        /// Use `Self::parameters`
        pub fn get_params(&self) -> &BTreeMap<String, Value> {
            self.parameters()
        }

        /// Sets the parameters `BTreeMap`, overriding current values
        pub fn set_parameters<V: Serialize>(&mut self, params: &BTreeMap<String, V>) {
            self.parameters = params.iter()
                .map(|(k, v)| (k.to_owned(), serde_json::value::to_value(&v)))
                .collect();
        }

        /// Use `Self::set_parameters`
        pub fn set_params<V: Serialize>(&mut self, params: &BTreeMap<String, V>) {
            self.set_parameters(params);
        }

        /// Removes parameter from the statment
        ///
        /// Trying to remove a non-existent parameter has no effect
        pub fn remove_param(&mut self, key: &str) {
            self.parameters.remove(key);
        }
    }
}

#[cfg(feature = "rustc-serialize")]
mod inner {
    use std::collections::BTreeMap;
    use std::error::Error;
    use rustc_serialize::{Encodable, Decodable};
    use rustc_serialize::json as rustc_json;
    use serde_json::{self, Value};
    use ::error::GraphError;

    /// Represents a statement to be sent to the server
    #[derive(Clone, Debug, Serialize)]
    pub struct Statement {
        statement: String,
        parameters: BTreeMap<String, Value>,
        #[serde(skip_serializing)]
        param_errors: Vec<(String, String)>,
    }

    impl Statement  {
        pub fn new(statement: &str) -> Self {
            Statement {
                statement: statement.to_owned(),
                parameters: BTreeMap::new(),
                param_errors: Vec::new(),
            }
        }

        /// Returns the statement text
        pub fn statement(&self) -> &str {
            &self.statement
        }

        /// Adds parameter in builder style
        ///
        /// This method consumes `self` and returns it with the parameter added, so the binding does
        /// not need to be mutable
        ///
        /// # Examples
        ///
        /// ```
        /// # use rusted_cypher::Statement;
        /// let statement = Statement::new("MATCH n RETURN n")
        ///     .with_param("param1", "value1")
        ///     .with_param("param2", 2)
        ///     .with_param("param3", 3.0);
        /// ```
        pub fn with_param<V: Encodable + Copy>(mut self, key: &str, value: V) -> Self {
            self.add_param(key, value);
            self
        }

        /// Adds parameter to the `Statement`
        pub fn add_param<V: Encodable + Copy>(&mut self, key: &str, value: V) {
            let between = match rustc_json::encode(&value) {
                Ok(value) => value,
                Err(e) => {
                    self.param_errors.push((key.to_owned(), format!("{}", e)));
                    return
                }
            };

            let value = match serde_json::from_str::<Value>(&between) {
                Ok(value) => value,
                Err(e) => {
                    self.param_errors.push((key.to_owned(), format!("{}", e)));
                    return
                }
            };

            self.parameters.insert(key.to_owned(), value);
        }

        /// Gets the value of the parameter
        ///
        /// Returns `None` if there is no parameter with the given name or `Some(serde_json::error::Error)``
        /// if the parameter cannot be converted back from `serde_json::value::Value`
        pub fn param<V: Decodable>(&self, key: &str) -> Option<Result<V, GraphError>> {
            self.parameters.get(key.into()).map(|v| {
                let between = match serde_json::to_string(&v) {
                    Ok(value) => value,
                    Err(e) => return Err(GraphError::new_error(Box::new(e))),
                };
                rustc_json::decode(&between).map_err(From::from)
            })
        }

        /// Use `Self::param`
        pub fn get_param<V: Decodable>(&self, key: &str) -> Option<Result<V, GraphError>> {
            self.param(key)
        }

        /// Gets a reference to the underlying parameters `BTreeMap`
        pub fn parameters(&self) -> &BTreeMap<String, Value> {
            &self.parameters
        }

        /// Use `Self::parameters`
        pub fn get_params(&self) -> &BTreeMap<String, Value> {
            self.parameters()
        }

        /// Sets the parameters `BTreeMap`, overriding current values
        pub fn set_parameters<V: Encodable>(&mut self, params: &BTreeMap<String, V>) -> Result<(), Box<Error>> {
            let mut new_params: BTreeMap<String, Value> = BTreeMap::new();

            for (k, v) in params.iter() {
                let between = try!(rustc_json::encode(&v));
                let value: Value = try!(serde_json::from_str(&between));
                new_params.insert(k.to_owned(), value);
            }

            Ok(())
        }

        /// Use `Self::set_parameters`
        pub fn set_params<V: Encodable>(&mut self, params: &BTreeMap<String, V>) -> Result<(), Box<Error>> {
            self.set_parameters(params)
        }

        /// Removes parameter from the statment
        ///
        /// Trying to remove a non-existent parameter has no effect
        pub fn remove_param(&mut self, key: &str) {
            self.parameters.remove(key);
        }

        pub fn has_param_errors(&self) -> bool {
            !self.param_errors.is_empty()
        }

        pub fn param_errors(&self) -> &Vec<(String, String)> {
            &self.param_errors
        }
    }
}

pub use self::inner::Statement;

impl<'a> From<&'a str> for Statement {
    fn from(stmt: &str) -> Self {
        Statement::new(stmt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(unused_variables)]
    fn from_str() {
        let stmt = Statement::new("MATCH n RETURN n");
    }

    #[test]
    fn with_param() {
        let statement = Statement::new("MATCH n RETURN n")
            .with_param("param1", "value1")
            .with_param("param2", 2)
            .with_param("param3", 3.0)
            .with_param("param4", [0; 4]);

        assert_eq!(statement.parameters().len(), 4);
    }

    #[test]
    fn add_param() {
        let mut statement = Statement::new("MATCH n RETURN n");
        statement.add_param("param1", "value1");
        statement.add_param("param2", 2);
        statement.add_param("param3", 3.0);
        statement.add_param("param4", [0; 4]);

        assert_eq!(statement.parameters().len(), 4);
    }

    #[test]
    fn remove_param() {
        let mut statement = Statement::new("MATCH n RETURN n")
            .with_param("param1", "value1")
            .with_param("param2", 2)
            .with_param("param3", 3.0)
            .with_param("param4", [0; 4]);

        statement.remove_param("param1");

        assert_eq!(statement.parameters().len(), 3);
    }

    #[test]
    #[allow(unused_variables)]
    fn macro_without_params() {
        let stmt = cypher_stmt!("MATCH n RETURN n");
    }

    #[test]
    fn macro_single_param() {
        let statement1 = cypher_stmt!("MATCH n RETURN n" {
            "name" => "test"
        });

        let param = 1;
        let statement2 = cypher_stmt!("MATCH n RETURN n" {
            "value" => param
        });

        assert_eq!("test", statement1.param::<String>("name").unwrap().unwrap());
        assert_eq!(param, statement2.param::<i32>("value").unwrap().unwrap());
    }

    #[test]
    fn macro_multiple_params() {
        let param = 3f32;
        let statement = cypher_stmt!("MATCH n RETURN n" {
            "param1" => "one",
            "param2" => 2,
            "param3" => param
        });

        assert_eq!("one", statement.param::<String>("param1").unwrap().unwrap());
        assert_eq!(2, statement.param::<i32>("param2").unwrap().unwrap());
        assert_eq!(param, statement.param::<f32>("param3").unwrap().unwrap());
    }
}
