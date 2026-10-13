import pytest
from precision_reference import MP, matrix, projective_error, qasm_operations
from tzap import optimize_qasm


@pytest.mark.parametrize("family", ["cp", "crx", "cry", "crz"])
@pytest.mark.parametrize("reverse", [False, True])
@pytest.mark.parametrize(
    "expression", ["exp(128)", "exp(256)", "sin(exp(256))", "cos(exp(256))"]
)
def test_controlled_symbolic_synthesis_adapts_certification_precision(
    family, reverse, expression
):
    bits = [1, 0] if reverse else [0, 1]
    source = f"qreg q[2]; {family}({expression}) q[{bits[0]}],q[{bits[1]}];"
    result = optimize_qasm(source, passes=["DecomposeRotations"], rz_epsilon=1e-5)
    output = qasm_operations(result.qasm)
    assert all(not parameters for _, parameters, _ in output)
    # Evaluate the original function independently, retaining enough precision
    # to reduce exp(256)'s 112-digit magnitude without a binary64 conversion.
    value = MP.exp(128 if expression == "exp(128)" else 256)
    if expression.startswith("sin"):
        value = MP.sin(value)
    elif expression.startswith("cos"):
        value = MP.cos(value)
    expected = matrix([(family, [value], bits)], 2)
    assert projective_error(expected, matrix(output, 2)) <= MP.mpf("1e-5")
    assert result.report.numerical.certified_syntheses == 1
    assert result.report.numerical.uncertified_syntheses == 0
