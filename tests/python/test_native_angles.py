import numpy as np
import pennylane as qml
import pytest
from qiskit import QuantumCircuit
from qiskit.quantum_info import Operator
from tzap._angles import angle_radians, exact_float_expression
from tzap.pennylane import optimize as optimize_tape
from tzap.qiskit import optimize


@pytest.mark.parametrize("axis", ["rx", "ry", "p"])
def test_qiskit_rebuilds_symbolic_native_angles(axis):
    circuit = QuantumCircuit(1)
    circuit.t(0)
    if axis in ("rx", "ry"):
        circuit.h(0)
    if axis == "ry":
        circuit.s(0)
    getattr(circuit, axis)(0.125, 0)
    result = optimize(circuit, passes=["PhaseFoldPauli"])
    assert Operator(circuit).equiv(Operator(result))


@pytest.mark.parametrize("axis", ["rx", "ry", "p"])
def test_pennylane_rebuilds_symbolic_native_angles(axis):
    gates = [qml.T(0)]
    if axis in ("rx", "ry"):
        gates.append(qml.Hadamard(0))
    if axis == "ry":
        gates.append(qml.S(0))
    kind = {"rx": qml.RX, "ry": qml.RY, "p": qml.PhaseShift}[axis]
    gates.append(kind(0.125, wires=0))
    tape = qml.tape.QuantumScript(gates)
    (result,), _ = optimize_tape(tape, passes=["PhaseFoldPauli"])
    before, after = qml.matrix(tape), qml.matrix(result)
    overlap = np.vdot(before, after)
    assert np.allclose(after, before * overlap / abs(overlap), atol=1e-12)


def test_emitted_angle_conversion_accepts_only_finite_literals():
    assert angle_radians("0.125") == 0.125
    assert angle_radians("1e17") == 1e17
    for expression in [
        "(pi/4)+(1e17)",
        "pi/7",
        "nan",
        "inf",
        "abs(1)",
        "__import__('os')",
        "pi.real",
        "2**3",
        "'1'",
    ]:
        with pytest.raises(ValueError):
            angle_radians(expression)


def _operations(axis, value):
    operations = [("t", [], [0])]
    if axis in ("rx", "ry"):
        operations.append(("h", [], [0]))
    if axis == "ry":
        operations.append(("s", [], [0]))
    operations.append((axis, [value], [0]))
    if axis == "ry":
        operations.append(("sdg", [], [0]))
    if axis in ("rx", "ry"):
        operations.append(("h", [], [0]))
    return operations


def _qiskit_operations(circuit):
    return [
        (
            op.operation.name,
            op.operation.params,
            [circuit.find_bit(q).index for q in op.qubits],
        )
        for op in circuit.data
    ]


def _pennylane_operations(tape):
    names = {
        "T": "t",
        "Adjoint(T)": "tdg",
        "S": "s",
        "Adjoint(S)": "sdg",
        "PauliZ": "z",
        "Hadamard": "h",
        "PauliX": "x",
        "PhaseShift": "p",
        "RX": "rx",
        "RY": "ry",
        "RZ": "rz",
        "ControlledPhaseShift": "cp",
        "CRX": "crx",
        "CRY": "cry",
        "CRZ": "crz",
        "CNOT": "cx",
        "CZ": "cz",
    }
    return [(names[op.name], op.parameters, list(op.wires)) for op in tape.operations]


@pytest.mark.parametrize("adapter", ["qiskit", "pennylane"])
@pytest.mark.parametrize("axis", ["p", "rx", "ry", "rz"])
@pytest.mark.parametrize(
    "value", [1e17, -1e17, 0.125, float.fromhex("0x1.fffffffffffffp+1023")]
)
def test_mixed_angles_preserve_independent_reference(adapter, axis, value):
    from precision_reference import MP, matrix, projective_error, x_plus_probability

    operations = _operations(axis, value)
    if adapter == "qiskit":
        circuit = QuantumCircuit(1)
        for name, params, bits in operations:
            getattr(circuit, name)(*params, *bits)
        result = optimize(circuit, passes=["PhaseFoldPauli"])
        output = _qiskit_operations(result)
    else:
        kinds = {
            "p": qml.PhaseShift,
            "rx": qml.RX,
            "ry": qml.RY,
            "rz": qml.RZ,
            "t": qml.T,
            "h": qml.Hadamard,
            "s": qml.S,
            "sdg": qml.adjoint(qml.S),
        }
        tape = qml.tape.QuantumScript(
            [kinds[name](*params, wires=bits) for name, params, bits in operations]
        )
        (result,), _ = optimize_tape(tape, passes=["PhaseFoldPauli"])
        output = _pennylane_operations(result)
    before, after = matrix(operations, 1), matrix(output, 1)
    assert projective_error(before, after) < MP.mpf("1e-90")
    assert abs(x_plus_probability(before) - x_plus_probability(after)) < MP.mpf("1e-90")


def test_precision_reference_detects_the_lost_quarter_turn():
    from precision_reference import MP, matrix, projective_error, x_plus_probability

    before = matrix(_operations("p", 1e17), 1)
    wrong = matrix([("p", [1e17], [0])], 1)
    assert projective_error(before, wrong) > MP.mpf("0.3")
    error_pp = 100 * abs(x_plus_probability(before) - x_plus_probability(wrong))
    assert abs(error_pp - MP.mpf("29.3923061761662089")) < MP.mpf("1e-15")


def _optimize_operations(adapter, operations, qubits, **options):
    if adapter == "qiskit":
        circuit = QuantumCircuit(qubits)
        for name, params, bits in operations:
            getattr(circuit, name)(*params, *bits)
        return _qiskit_operations(optimize(circuit, **options))
    kinds = {
        "p": qml.PhaseShift,
        "rx": qml.RX,
        "ry": qml.RY,
        "rz": qml.RZ,
        "t": qml.T,
        "tdg": qml.adjoint(qml.T),
        "h": qml.Hadamard,
        "s": qml.S,
        "sdg": qml.adjoint(qml.S),
        "cp": qml.ControlledPhaseShift,
        "crx": qml.CRX,
        "cry": qml.CRY,
        "crz": qml.CRZ,
        "cx": qml.CNOT,
    }
    tape = qml.tape.QuantumScript(
        [kinds[name](*params, wires=bits) for name, params, bits in operations]
    )
    (result,), _ = optimize_tape(tape, **options)
    return _pennylane_operations(result)


@pytest.mark.parametrize("adapter", ["qiskit", "pennylane"])
@pytest.mark.parametrize("axis", ["p", "rx", "ry", "rz"])
@pytest.mark.parametrize("level", ["O1", "O2", "O3"])
@pytest.mark.parametrize("parallel", [False, True])
def test_large_mixed_angles_across_default_pipelines(adapter, axis, level, parallel):
    from precision_reference import MP, matrix, projective_error

    operations = _operations(axis, 1e17)
    output = _optimize_operations(
        adapter, operations, 1, level=level, parallel=parallel
    )
    assert projective_error(matrix(operations, 1), matrix(output, 1)) < MP.mpf("1e-90")


@pytest.mark.parametrize("adapter", ["qiskit", "pennylane"])
@pytest.mark.parametrize("axis", ["cp", "crx", "cry", "crz"])
@pytest.mark.parametrize(
    "value", [1e17, -1e17, 0.3, float.fromhex("0x0.0000000000001p-1022")]
)
@pytest.mark.parametrize("reverse", [False, True])
def test_controlled_parameters_keep_literal_values_and_branch_phases(
    adapter, axis, value, reverse
):
    from precision_reference import MP, matrix, projective_error

    bits = [1, 0] if reverse else [0, 1]
    operations = [
        ("h", [], [0]),
        ("h", [], [1]),
        (axis, [value], bits),
        ("t", [], [bits[1]]),
    ]
    output = _optimize_operations(adapter, operations, 2, level="O3")
    assert projective_error(matrix(operations, 2), matrix(output, 2)) < MP.mpf("1e-90")
    assert any(
        name in ("cp", "crz")
        and float(parameters[0]).as_integer_ratio() == value.as_integer_ratio()
        for name, parameters, _ in output
    )


@pytest.mark.parametrize("adapter", ["qiskit", "pennylane"])
@pytest.mark.parametrize("axis", ["cp", "crx", "cry", "crz"])
@pytest.mark.parametrize(
    "value",
    [
        1e17,
        -1e17,
        float.fromhex("0x0.0000000000001p-1022"),
        -float.fromhex("0x0.0000000000001p-1022"),
    ],
)
def test_controlled_synthesis_matches_independent_high_precision_matrix(
    adapter, axis, value
):
    from precision_reference import MP, matrix, projective_error

    operations = [(axis, [value], [1, 0])]
    output = _optimize_operations(
        adapter, operations, 2, passes=["DecomposeRotations"], rz_epsilon=1e-4
    )
    assert not any(
        name in ("cp", "crx", "cry", "crz", "p", "rx", "ry", "rz")
        for name, _, _ in output
    )
    assert projective_error(matrix(operations, 2), matrix(output, 2)) < MP.mpf("1e-4")


@pytest.mark.parametrize("axis", ["cp", "crx", "cry", "crz"])
@pytest.mark.parametrize("numerator", [1, -1])
def test_synthesis_promotes_pi_coefficients_when_halved(axis, numerator):
    from fractions import Fraction

    from precision_reference import (
        MP,
        PiMultiple,
        matrix,
        projective_error,
        qasm_operations,
    )
    from tzap import _native, optimize_qasm

    angle = PiMultiple(Fraction(numerator, 9223372036854775807))
    source = f"qreg q[2]; {axis}({numerator}*pi/9223372036854775807) q[1],q[0];"
    result = optimize_qasm(source, passes=["DecomposeRotations"], rz_epsilon=1e-20)
    output = qasm_operations(_native._sdk_qasm(result.qasm))
    assert projective_error(
        matrix([(axis, [angle], [1, 0])], 2), matrix(output, 2)
    ) < MP.mpf("1e-20")
    assert result.report.numerical.certified_syntheses == 1


@pytest.mark.parametrize("axis", ["cp", "crx", "cry", "crz"])
def test_sdk_exact_export_retains_controlled_full_turn_phase(axis):
    from fractions import Fraction

    from precision_reference import (
        MP,
        PiMultiple,
        matrix,
        projective_error,
        qasm_operations,
    )
    from tzap import _native

    for numerator in [2, -2, 4, -4]:
        source = f"qreg q[2]; {axis}({numerator}*pi) q[1],q[0];"
        output = qasm_operations(_native._sdk_qasm(source))
        expected = matrix([(axis, [PiMultiple(Fraction(numerator))], [1, 0])], 2)
        assert projective_error(expected, matrix(output, 2)) < MP.mpf("1e-90")


@pytest.mark.parametrize("axis", ["cp", "crx", "cry", "crz"])
@pytest.mark.parametrize("reverse", [False, True])
@pytest.mark.parametrize(
    "value", [1e17, -1e17, float.fromhex("0x1.fffffffffffffp+1023")]
)
def test_sdk_exact_export_preserves_mixed_controlled_angles(axis, reverse, value):
    from fractions import Fraction

    from precision_reference import (
        MP,
        PiMultiple,
        matrix,
        projective_error,
        qasm_operations,
    )
    from tzap import _native

    bits = [1, 0] if reverse else [0, 1]
    source = f"qreg q[2]; {axis}((pi/2)+({exact_float_expression(value)})) q[{bits[0]}],q[{bits[1]}];"
    output = qasm_operations(_native._sdk_qasm(source))
    # Same-axis factors define the reference without adding pi to a float.
    expected = matrix(
        [(axis, [PiMultiple(Fraction(1, 2))], bits), (axis, [value], bits)], 2
    )
    assert projective_error(expected, matrix(output, 2)) < MP.mpf("1e-90")


@pytest.mark.parametrize("axis", ["cp", "crx", "cry", "crz"])
def test_sdk_export_refuses_mixed_controlled_angles_with_inexact_halves(axis):
    from tzap import OptimizationError, _native

    for angle in ["(pi/2)+(5e-324)", "pi/4"]:
        source = f"qreg q[2]; {axis}({angle}) q[0],q[1];"
        with pytest.raises(OptimizationError, match="gate 0"):
            _native._sdk_qasm(source)


def test_sdk_export_rejects_unsupported_symbolic_angles_and_preserved_expressions():
    from tzap import OptimizationError, _native

    for axis in ["p", "rx", "ry", "rz", "cp", "crx", "cry", "crz"]:
        operands = "q[0],q[1]" if axis.startswith("c") else "q[0]"
        for angle in ["pi/7", "pi/9223372036854775808"]:
            with pytest.raises(OptimizationError, match="gate 1"):
                _native._sdk_qasm(f"qreg q[2]; h q[0]; {axis}({angle}) {operands};")


def test_mixed_axes_and_cancellation_keep_high_precision_semantics():
    from precision_reference import MP, matrix, projective_error

    operations = [
        ("t", [], [0]),
        ("p", [1e17], [0]),
        ("p", [-1e17], [0]),
        ("rx", [0.125], [0]),
        ("ry", [-0.375], [0]),
        ("rz", [1e17], [0]),
        ("rz", [-1e17], [0]),
        ("tdg", [], [0]),
    ]
    for adapter in ["qiskit", "pennylane"]:
        for parallel in [False, True]:
            output = _optimize_operations(
                adapter, operations, 1, level="O3", parallel=parallel
            )
            assert projective_error(matrix(operations, 1), matrix(output, 1)) < MP.mpf(
                "1e-90"
            )
