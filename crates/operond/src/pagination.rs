use tonic::Status;

pub(crate) fn paginate_deque<T: Clone>(
    items: &std::collections::VecDeque<T>,
    page_size: u32,
    page_token: &str,
) -> Result<(Vec<T>, String), Status> {
    let (start, end, next) = page_bounds(items.len(), page_size, page_token)?;
    Ok((
        items
            .iter()
            .skip(start)
            .take(end - start)
            .cloned()
            .collect(),
        next,
    ))
}

pub(crate) fn paginate_items<T: Clone>(
    items: &[T],
    page_size: u32,
    page_token: &str,
) -> Result<(Vec<T>, String), Status> {
    let (start, end, next) = page_bounds(items.len(), page_size, page_token)?;
    Ok((items[start..end].to_vec(), next))
}

pub(crate) fn page_bounds(
    length: usize,
    page_size: u32,
    page_token: &str,
) -> Result<(usize, usize, String), Status> {
    let start = if page_token.is_empty() {
        0
    } else {
        page_token
            .parse::<usize>()
            .map_err(|_| Status::invalid_argument("page_token must be an item offset"))?
    };
    if start > length {
        return Err(Status::invalid_argument("page_token is out of range"));
    }
    if page_size == 0 {
        return Ok((start, length, String::new()));
    }
    let size = usize::min(page_size as usize, 1000);
    let end = usize::min(start.saturating_add(size), length);
    let next = if end < length {
        end.to_string()
    } else {
        String::new()
    };
    Ok((start, end, next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_page_clones_only_requested_records() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        struct Counted(usize, Arc<AtomicUsize>);
        impl Clone for Counted {
            fn clone(&self) -> Self {
                self.1.fetch_add(1, Ordering::SeqCst);
                Self(self.0, self.1.clone())
            }
        }
        let clones = Arc::new(AtomicUsize::new(0));
        let items = (0..10000)
            .map(|index| Counted(index, clones.clone()))
            .collect();
        let (page, next) = paginate_deque(&items, 10, "100").unwrap();
        assert_eq!(clones.load(Ordering::SeqCst), 10);
        assert_eq!(
            page.iter().map(|item| item.0).collect::<Vec<_>>(),
            (100..110).collect::<Vec<_>>()
        );
        assert_eq!(next, "110");
        assert!(paginate_deque(&items, 10, "10001").is_err());
        assert_eq!(clones.load(Ordering::SeqCst), 10);
        assert!(paginate_deque(&items, 10, "10000").unwrap().0.is_empty());
    }

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
