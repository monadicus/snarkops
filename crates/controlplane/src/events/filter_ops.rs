use super::{EventFilter, EventKindFilter};

impl std::ops::BitAnd for EventFilter {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (EventFilter::Unfiltered, rhs) => rhs,
            (lhs, EventFilter::Unfiltered) => lhs,
            (EventFilter::AllOf(mut filters), EventFilter::AllOf(rhs_filters)) => {
                filters.extend(rhs_filters);
                EventFilter::AllOf(filters)
            }
            (EventFilter::AllOf(mut filters), rhs) => {
                filters.push(rhs);
                EventFilter::AllOf(filters)
            }
            (lhs, EventFilter::AllOf(mut rhs_filters)) => {
                rhs_filters.push(lhs);
                EventFilter::AllOf(rhs_filters)
            }
            (lhs, rhs) => EventFilter::AllOf(vec![lhs, rhs]),
        }
    }
}

impl std::ops::BitOr for EventFilter {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (EventFilter::Unfiltered, _) => EventFilter::Unfiltered,
            (_, EventFilter::Unfiltered) => EventFilter::Unfiltered,
            (EventFilter::AnyOf(mut filters), EventFilter::AnyOf(rhs_filters)) => {
                filters.extend(rhs_filters);
                EventFilter::AnyOf(filters)
            }
            (EventFilter::AnyOf(mut filters), rhs) => {
                filters.push(rhs);
                EventFilter::AnyOf(filters)
            }
            (lhs, EventFilter::AnyOf(mut rhs_filters)) => {
                rhs_filters.push(lhs);
                EventFilter::AnyOf(rhs_filters)
            }
            (lhs, rhs) => EventFilter::AnyOf(vec![lhs, rhs]),
        }
    }
}

impl std::ops::BitXor for EventFilter {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (EventFilter::Unfiltered, rhs) => rhs,
            (lhs, EventFilter::Unfiltered) => lhs,
            (EventFilter::OneOf(mut filters), EventFilter::OneOf(rhs_filters)) => {
                filters.extend(rhs_filters);
                EventFilter::OneOf(filters)
            }
            (EventFilter::OneOf(mut filters), rhs) => {
                filters.push(rhs);
                EventFilter::OneOf(filters)
            }
            (lhs, EventFilter::OneOf(mut rhs_filters)) => {
                rhs_filters.push(lhs);
                EventFilter::OneOf(rhs_filters)
            }
            (lhs, rhs) => EventFilter::OneOf(vec![lhs, rhs]),
        }
    }
}

impl std::ops::Not for EventFilter {
    type Output = Self;

    fn not(self) -> Self::Output {
        EventFilter::Not(Box::new(self))
    }
}

impl std::ops::Not for EventKindFilter {
    type Output = EventFilter;

    fn not(self) -> Self::Output {
        !EventFilter::EventIs(self)
    }
}

impl std::ops::BitOr<EventFilter> for EventKindFilter {
    type Output = EventFilter;

    fn bitor(self, rhs: EventFilter) -> Self::Output {
        EventFilter::EventIs(self) | rhs
    }
}

impl std::ops::BitAnd<EventFilter> for EventKindFilter {
    type Output = EventFilter;

    fn bitand(self, rhs: EventFilter) -> Self::Output {
        EventFilter::EventIs(self) & rhs
    }
}

impl std::ops::BitXor<EventFilter> for EventKindFilter {
    type Output = EventFilter;

    fn bitxor(self, rhs: EventFilter) -> Self::Output {
        EventFilter::EventIs(self) ^ rhs
    }
}

impl std::ops::BitOr<EventKindFilter> for EventFilter {
    type Output = EventFilter;

    fn bitor(self, rhs: EventKindFilter) -> Self::Output {
        self | EventFilter::EventIs(rhs)
    }
}

impl std::ops::BitAnd<EventKindFilter> for EventFilter {
    type Output = EventFilter;

    fn bitand(self, rhs: EventKindFilter) -> Self::Output {
        self & EventFilter::EventIs(rhs)
    }
}

impl std::ops::BitXor<EventKindFilter> for EventFilter {
    type Output = EventFilter;

    fn bitxor(self, rhs: EventKindFilter) -> Self::Output {
        self ^ EventFilter::EventIs(rhs)
    }
}

impl std::ops::BitOr for EventKindFilter {
    type Output = EventFilter;

    fn bitor(self, rhs: Self) -> Self::Output {
        EventFilter::EventIs(self) | EventFilter::EventIs(rhs)
    }
}

impl std::ops::BitAnd for EventKindFilter {
    type Output = EventFilter;

    fn bitand(self, rhs: Self) -> Self::Output {
        EventFilter::EventIs(self) & EventFilter::EventIs(rhs)
    }
}

impl std::ops::BitXor for EventKindFilter {
    type Output = EventFilter;

    fn bitxor(self, rhs: Self) -> Self::Output {
        EventFilter::EventIs(self) ^ EventFilter::EventIs(rhs)
    }
}
