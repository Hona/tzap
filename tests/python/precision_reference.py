"""Independent matrices from exact binary64 rationals and mathematical pi.

Never call SDK matrices or tzap's angle conversion. Use a private mpmath
context so test order cannot change the reference precision.
"""

from dataclasses import dataclass
from fractions import Fraction

import mpmath

MP = mpmath.mp.clone()
MP.dps = 420


@dataclass(frozen=True)
class PiMultiple:
    coefficient: Fraction


def binary(value):
    if isinstance(value, PiMultiple):
        return MP.pi * value.coefficient.numerator / value.coefficient.denominator
    numerator, denominator = float(value).as_integer_ratio()
    return MP.mpf(numerator) / denominator


def matrix(operations, qubits):
    """Operations are (name, numeric parameters, little-endian bit indices)."""
    result = MP.eye(1 << qubits)
    for name, parameters, bits in operations:
        control = None
        if name in ("cp", "crx", "cry", "crz", "cx", "cz"):
            control, target = bits
            name = {"cp": "p", "cx": "x", "cz": "z"}.get(name, name[1:])
        else:
            (target,) = bits
        angle = binary(parameters[0]) if parameters else None
        if name in ("t", "tdg", "s", "sdg", "z"):
            turns = {"t": 1, "tdg": -1, "s": 2, "sdg": -2, "z": 4}[name]
            single = MP.matrix([[1, 0], [0, MP.exp(1j * MP.pi * turns / 4)]])
        elif name == "h":
            single = MP.matrix([[1, 1], [1, -1]]) / MP.sqrt(2)
        elif name == "x":
            single = MP.matrix([[0, 1], [1, 0]])
        elif name == "p":
            single = MP.matrix([[1, 0], [0, MP.exp(1j * angle)]])
        elif name == "rz":
            single = MP.matrix(
                [[MP.exp(-1j * angle / 2), 0], [0, MP.exp(1j * angle / 2)]]
            )
        elif name == "rx":
            c, s = MP.cos(angle / 2), MP.sin(angle / 2)
            single = MP.matrix([[c, -1j * s], [-1j * s, c]])
        elif name == "ry":
            c, s = MP.cos(angle / 2), MP.sin(angle / 2)
            single = MP.matrix([[c, -s], [s, c]])
        else:
            raise AssertionError(f"unsupported reference operation {name}")
        gate = MP.eye(1 << qubits)
        for zero in range(1 << qubits):
            if zero & (1 << target) or (
                control is not None and not zero & (1 << control)
            ):
                continue
            one = zero | (1 << target)
            for i, row in enumerate((zero, one)):
                for j, column in enumerate((zero, one)):
                    gate[row, column] = single[i, j]
        result = gate * result
    return result


def projective_error(before, after):
    index = max(
        ((i, j) for i in range(before.rows) for j in range(before.cols)),
        key=lambda ij: abs(before[ij]),
    )
    ratio = after[index] / before[index]
    assert abs(ratio) > 0
    phase = ratio / abs(ratio)
    return max(
        abs(after[i, j] - phase * before[i, j])
        for i in range(before.rows)
        for j in range(before.cols)
    )


def x_plus_probability(unitary):
    """Prepare |+>, apply the circuit, measure X; return P(+1)."""
    return abs(sum(unitary[i, j] for i in range(2) for j in range(2)) / 2) ** 2


def qasm_operations(source):
    """Read only safe export's fixed gates and literal numeric parameters."""
    import re

    operations = []
    for line in source.splitlines():
        if line.startswith(("OPENQASM", "include", "qreg", "creg")) or not line.strip():
            continue
        token, operands = line.rstrip(";").split(" ", 1)
        if "(" in token:
            name, parameter = token.split("(", 1)
            parameters = [float(parameter[:-1])]
        else:
            name, parameters = token, []
        name = {"u1": "p", "cu1": "cp"}.get(name, name)
        operations.append(
            (name, parameters, [int(x) for x in re.findall(r"q\[(\d+)\]", operands)])
        )
    return operations
