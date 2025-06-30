#[cfg(test)]
mod test {
    use crate::rocket;
    use rocket::local::blocking::Client;
    use rocket::http::Status;

    struct Fixture {
        client: Client,
    }

    impl Fixture {
        fn new() -> Self {
            crate::test::logging::init_log();
            
            let client = Client::tracked(crate::rocket(/* TODO: fixture for miden client */)).expect("valid rocket instance");
            
            Fixture {
                client,
            }
        }
    }

    #[test]
    fn test_add_note() {
        // arrange
        let fixture = Fixture::new();
        
        // act
        let mut response = client.get(uri!(super::hello)).dispatch();
        
        // assert
        assert_eq!(response.status(), Status::Ok);
    }
}
