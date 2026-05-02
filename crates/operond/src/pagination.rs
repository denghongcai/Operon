use tonic::Status;

pub(crate) fn paginate_items<T: Clone>(
    items: &[T],
    page_size: u32,
    page_token: &str,
) -> Result<(Vec<T>, String), Status> {
    let start = if page_token.is_empty() {
        0
    } else {
        page_token
            .parse::<usize>()
            .map_err(|_| Status::invalid_argument("page_token must be an item offset"))?
    };
    if start > items.len() {
        return Err(Status::invalid_argument("page_token is out of range"));
    }
    if page_size == 0 {
        return Ok((items[start..].to_vec(), String::new()));
    }
    let size = usize::min(page_size as usize, 1000);
    let end = usize::min(start.saturating_add(size), items.len());
    let next = if end < items.len() {
        end.to_string()
    } else {
        String::new()
    };
    Ok((items[start..end].to_vec(), next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_deterministic_pages() {
        let items = vec![1, 2, 3, 4, 5];

        let (first, first_token) = paginate_items(&items, 2, "").expect("first page");
        assert_eq!(first, vec![1, 2]);
        assert_eq!(first_token, "2");

        let (second, second_token) = paginate_items(&items, 2, &first_token).expect("second page");
        assert_eq!(second, vec![3, 4]);
        assert_eq!(second_token, "4");

        let (last, last_token) = paginate_items(&items, 2, &second_token).expect("last page");
        assert_eq!(last, vec![5]);
        assert!(last_token.is_empty());
    }

    #[test]
    fn rejects_invalid_tokens() {
        let items = vec![1, 2, 3];

        assert_eq!(
            paginate_items(&items, 2, "not-a-number")
                .expect_err("invalid token")
                .code(),
            tonic::Code::InvalidArgument
        );
        assert_eq!(
            paginate_items(&items, 2, "4")
                .expect_err("out of range token")
                .code(),
            tonic::Code::InvalidArgument
        );
    }
}
