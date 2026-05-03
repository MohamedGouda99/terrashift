resource "aws_security_group" "db" {
  name = "db"
  vpc_id = aws_vpc.main.id
}

resource "aws_security_group" "ssh" {
  name = "ssh"
  vpc_id = aws_vpc.main.id
}

resource "aws_security_group" "web" {
  name = "web"
  vpc_id = aws_vpc.main.id
}
