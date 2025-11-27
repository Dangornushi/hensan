def fibonacci(n: int):
    fib_sequence = []
    a, b = 0, 1
    for _ in range(n):
        fib_sequence.append(a)
        a, b = b, a + b
    return fib_sequence

def main():
    # 最初の10個のフィボナッチ数を表示
    n = 10
    result = fibonacci(n)
    print(f"最初の{n}個のフィボナッチ数列:")
    print(result)

    # 個別に表示
    for i, num in enumerate(result):
        print(f"F({i}) = {num}")

if __name__ == "__main__":
    main()
