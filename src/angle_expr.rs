//! Closed QASM scalar expressions use exact decimals and mathematical pi.
pub(crate) fn parse(source: &str, line: usize) -> Result<crate::angle::Angle, String> {
    crate::angle::AngleContext::default()
        .parse(source)
        .map_err(|e| format!("line {line}: {e}"))
}
