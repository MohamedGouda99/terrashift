resource "aws_instance" "web" {
  instance_type = "t3.medium"
  ami = "ami-0c55b159cbfafe1f0"
  subnet_id = aws_subnet.main.id
}
