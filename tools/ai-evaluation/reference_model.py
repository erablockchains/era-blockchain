"""ERA reference arithmetic-mean forecaster v1. Untrained; no accuracy claim."""
def predict(observations):
    if not observations:
        raise ValueError('at least one observation required')
    return sum(value for _, value in observations) / len(observations)
