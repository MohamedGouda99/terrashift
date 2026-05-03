resource "aws_subnet" "private_a" {
  cidr_block = "10.0.10.0/24"
  vpc_id = aws_vpc.main.id
}

resource "aws_subnet" "public_a" {
  cidr_block = "10.0.1.0/24"
  vpc_id = aws_vpc.main.id
}

resource "aws_subnet" "public_b" {
  cidr_block = "10.0.2.0/24"
  vpc_id = aws_vpc.main.id
}
