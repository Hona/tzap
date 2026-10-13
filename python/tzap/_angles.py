"""Safe native export and literal-only reconstruction of SDK angles."""

import math

from . import _native


def sdk_qasm(source: str) -> str:
    """Expand symbolic quarter turns without rounding them into SDK parameters."""
    return _native._sdk_qasm(source)


def angle_radians(source: str) -> float:
    """Accept only finite numeric literals emitted by the safe native export."""
    try:
        result = float(source)
    except ValueError as error:
        raise ValueError("SDK angle must be a finite numeric literal") from error
    if not math.isfinite(result):
        raise ValueError("SDK angle must be finite")
    return result


def exact_float_expression(value: float) -> str:
    """Encode the exact binary64 input, independently of decimal QASM semantics."""
    numerator, denominator = value.as_integer_ratio()
    return f"({numerator}/{denominator})"
