resource "aws_subnet" "a" {
  cidr_block = "10.0.1.0/24"
  vpc_id = aws_vpc.main.id
}
